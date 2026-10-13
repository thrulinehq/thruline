//! P6-5 (design record §3.7, §3.8) — **Check it again, without a terminal.**
//!
//! The run route is a door into the one runner this product has
//! ([`crate::check::run_check_at`], the `thruline check` wrapper). It is not a
//! second runner: every run started here is recorded by that wrapper, as an
//! ordinary check execution for the Change, under the ordinary execution
//! record (`run_execution`), with its binding, its count and its examined
//! inputs — so the T-3 applicability and freshness layers read it exactly as
//! they read a run typed in a terminal, and nothing here re-decides them.
//!
//! ## Addresses
//!
//! | method | path | what it does |
//! |---|---|---|
//! | GET  | `/api/run/token` | the per-launch token, to this page's own origin only |
//! | GET  | `/api/changes/<id>/run` | the run sheet: what will run, where, what it can and cannot establish, permission, the run going now (since C30 R-RUN, with what it has printed so far and how long it has run), the last outcome |
//! | POST | `/api/changes/<id>/run/permission` | a person allows checks in the Change's exact recorded worktree folder |
//! | POST | `/api/changes/<id>/run/permission/revoke` | a person takes that permission back (P6-5B) |
//! | POST | `/api/changes/<id>/run` | start one allow-listed check from the accepted profile |
//!
//! Stopping a run is the T-3-4 address (`/runs/<run>/cancel`), untouched.
//!
//! ## The five guards (amendment §Security), each with a test that fails
//! when it is removed
//!
//! 1. **Loopback only.** The listener is bound by `resolve_bind_address`,
//!    which refuses anything but the IPv4 loopback, and every connection from
//!    a non-loopback peer is refused before a byte is read.
//! 2. **Same origin and a per-launch token.** Any page in any browser can send
//!    a request to a localhost port. The two write addresses here refuse a
//!    request with no `Origin`, with a foreign one, with no token or with a
//!    wrong one. The token is 256 bits from the operating system's random
//!    source, minted when HQ starts, held in memory only, and served only to a
//!    same-origin read (the response carries no CORS header, so a foreign page
//!    cannot read it even when it asks).
//! 3. **The command is resolved here.** The request names a check id and,
//!    optionally, an environment id — both closed-set identifiers. A request
//!    carrying anything else (a command, arguments, a path, a variable) is
//!    refused whole, before the store is opened.
//! 4. **Nothing is installed, elevated or run from the repository.** The id
//!    must be in the Change's accepted check profile AND in
//!    [`crate::check::RUNNABLE_CHECKS`], whose argv is fixed. The program is
//!    looked up by name on a search path; nothing is downloaded, and no row
//!    runs a shell or a script. Every run is started with the program's
//!    offline switch on ([`crate::check::OFFLINE_SWITCHES`]), so a dependency
//!    missing from this machine fails the build rather than being fetched.
//!    **The program is never the repository's**
//!    (ADV-run-security-2): every folder of the search path is canonicalized,
//!    a folder inside the project (or a relative one, which the run would
//!    resolve inside it) is never searched as the program's home and never
//!    handed on to the run, and a program whose first match is there — or is
//!    a link to a file there — refuses the run before anything starts
//!    (`program_in_project`, naming the program). The sheet never offers a
//!    folder inside the project as a place to run. **Nor is the program a
//!    proxy hands the run to** (ADV-run-security-3): when the program found is
//!    rustup's proxy, rustup is asked, in the project's folder and with the
//!    run's environment, which program it would start — a toolchain file can
//!    name one the repository wrote — and that answer is held to the same rule.
//!    **Asking never waits on the repository** (ADV-run-security-5): every
//!    toolchain file rustup would read is looked at first without opening it,
//!    and one that is not a plain file refuses the run; rustup is asked in a
//!    process group of its own and given [`RUSTUP_DEADLINE`], past which the
//!    group is killed and reaped and the run is refused; and HQ stopping kills
//!    any question still going ([`RunRoute::stop_questions`]). **And what
//!    rustup reads is read here too, as data** (ADV-run-security-6), whether
//!    or not the program is recognised as the proxy: a toolchain file, the
//!    `RUSTUP_TOOLCHAIN` HQ was started with, or a directory override or
//!    default in rustup's settings that names a toolchain in the project, or
//!    one linked into it, refuses the run. **The run starts the toolchain
//!    that was checked** (ADV-run-security-7): its child is given
//!    `RUSTUP_TOOLCHAIN` naming the toolchain rustup answered with, which
//!    rustup puts before every toolchain file and override, so a file
//!    rewritten between the check and the start cannot hand the run elsewhere.
//! 5. **No permission, no run.** Permission is scoped to the Change's exact
//!    recorded worktree folder; a different worktree needs its own grant.
//!    Its cache lives at `.thruline/run-permission.json` in that folder,
//!    sealed with a key beside the ledger — so a file an agent wrote into
//!    the repository is not a permission Sam gave. Removing the folder removes
//!    the cache, while the act remains visible in the record. The file is read,
//!    written and removed only inside a `.thruline` opened without following
//!    a link, and never through a link at its own name (ADV-run-security-1),
//!    so a link an agent planted there cannot carry any of the three outside
//!    the project.
//!
//! ## The permission is in the record (P6-5B, R-2026-09-23-17)
//!
//! The file is a lookup cache, not the record. Allowing appends one row to the
//! hash-chained, append-only `run_permissions` table (`thruline_core::
//! run_permission`) BEFORE the file is written, and the file names that row;
//! taking it back appends one row pointing at the grant BEFORE the file is
//! removed. On every run request the file is read as a cache of the latest row
//! for this folder, and **the record wins**: a run goes only when the two
//! agree. A revocation refuses whatever the file says; a sealed file naming no
//! standing grant is refused; a standing grant whose file is gone is asked
//! again, because the file going away is how "the project was removed" has
//! always withdrawn it. The file can take permission away; only the record
//! can give it.
//!
//! A double-click starts one job: the check-and-insert on the project's job
//! slot happens under one lock, and a second start while one is going answers
//! with the run that is already going.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, Read};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{read_interpret_body, respond, RequestHeaders, State};
use crate::check::{self, RunnableCheck};

/// Bump when a served field of the run sheet changes.
///
/// 3 (C30 R-RUN): `job` gained `recorded_started_at`, `elapsed_s` and
/// `output`, and `checks[].last` gained `elapsed_s`. Additive; nothing that
/// was served moved.
pub(super) const RUN_VIEW_SCHEMA_VERSION: u32 = 3;

/// The header a write here must carry the per-launch token in.
pub(super) const TOKEN_HEADER: &str = "x-thruline-run-token";

/// Where the permission cache lives, under the exact recorded worktree folder.
pub(super) const PERMISSION_FILE: &str = ".thruline/run-permission.json";

/// The seal key's file name, beside the ledger.
const KEY_FILE: &str = "run-permission.key";

/// The promise the sheet makes, stored in the permission file too so the file
/// says what was agreed to.
const PROMISE: &str = "It does not install anything and does not touch the network.";

/// One environment a check can run in: the one HQ was started with, a folder
/// inside the project, or a well-known folder on this machine. Identified by
/// a closed-set id; the folder is decided here, never by a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Where {
    /// The search path HQ itself was started with.
    Hq,
    /// A folder under the project root.
    Project(&'static str),
    /// A folder under the home directory.
    Home(&'static str),
    /// An absolute folder on this machine.
    Machine(&'static str),
}

const ENVIRONMENTS: &[(&str, Where)] = &[
    ("hq", Where::Hq),
    ("project-venv", Where::Project(".venv/bin")),
    ("project-venv-plain", Where::Project("venv/bin")),
    ("project-node", Where::Project("node_modules/.bin")),
    ("home-cargo", Where::Home(".cargo/bin")),
    ("home-local", Where::Home(".local/bin")),
    ("machine-homebrew", Where::Machine("/opt/homebrew/bin")),
    ("machine-usr-local", Where::Machine("/usr/local/bin")),
];

/// A run this HQ started and has not yet seen end.
#[derive(Debug, Clone)]
struct Job {
    change_id: String,
    check: String,
    run_key: String,
    environment: &'static str,
    started_at: String,
    /// C30 R-RUN: what the run has printed so far, written by the wrapper's
    /// tee and read by the sheet. It goes when the job goes.
    live: Arc<check::LiveOutput>,
}

/// What became of a run that never reached the wrapper's first record.
#[derive(Debug, Clone)]
struct NotStarted {
    run_key: String,
    check: String,
    reason: String,
}

/// The run route's state for one HQ launch.
pub(super) struct RunRoute {
    token: Option<String>,
    /// One job per project folder. The lock is held across "is one going?"
    /// and "this one is going now", which is what makes a double-click one job.
    jobs: Mutex<BTreeMap<PathBuf, Job>>,
    /// Runs that ended before the wrapper recorded anything, by Change.
    not_started: Mutex<BTreeMap<String, NotStarted>>,
    /// The environment a project's last run used, for this launch only.
    remembered: Mutex<BTreeMap<PathBuf, &'static str>>,
    /// ADV-run-ledger-4: one consent act (allow, take back) at a time in this
    /// HQ, held from the read of where consent stands to the last write of its
    /// copy in the folder, so the record and its cache change as one step and
    /// a press cannot read "no copy yet" while another press is between its
    /// row and its file.
    consent_acts: Mutex<()>,
    /// The questions put to rustup that have not ended (ADV-run-security-5).
    questions: Mutex<Questions>,
    /// Test-only: the checks this route may start, in place of the allow-list.
    #[cfg(test)]
    catalogue: Option<&'static [RunnableCheck]>,
    /// Test-only: the search path HQ was started with and the home folder, in
    /// place of this process's own — so a test can put a folder on HQ's path,
    /// or a program in a home folder, without touching a real folder or this
    /// process's environment.
    #[cfg(test)]
    machine: Option<(std::ffi::OsString, PathBuf)>,
    /// Test-only: variables set (`Some`) or removed (`None`) for everything
    /// this route starts — the run's child, and the question put to rustup —
    /// over this process's own, so a test can give rustup a home of its own
    /// without touching this process's environment or the machine's rustup.
    #[cfg(test)]
    env: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>,
    /// Test-only: how long rustup is given to answer, in place of
    /// [`RUSTUP_DEADLINE`] — so a test can tell a question HQ's stop ended
    /// from one its deadline ended.
    #[cfg(test)]
    deadline: Option<Duration>,
}

/// The questions put to rustup that have not ended, and whether HQ is stopping.
#[derive(Debug, Default)]
struct Questions {
    /// Set once, when HQ stops: from then on rustup is never asked again.
    stopping: bool,
    /// The process group of each question still going. A question's own
    /// group, with its leader not yet reaped — a group is removed from here
    /// only under this lock and only once its leader is reaped, so a group
    /// named here is always this HQ's to signal.
    groups: BTreeSet<i32>,
}

impl RunRoute {
    pub(super) fn new() -> Self {
        Self {
            token: mint_token(),
            jobs: Mutex::new(BTreeMap::new()),
            not_started: Mutex::new(BTreeMap::new()),
            remembered: Mutex::new(BTreeMap::new()),
            consent_acts: Mutex::new(()),
            questions: Mutex::new(Questions::default()),
            #[cfg(test)]
            catalogue: None,
            #[cfg(test)]
            machine: None,
            #[cfg(test)]
            env: Vec::new(),
            #[cfg(test)]
            deadline: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_catalogue(catalogue: &'static [RunnableCheck]) -> Self {
        Self {
            catalogue: Some(catalogue),
            ..Self::new()
        }
    }

    /// Test-only: a route whose HQ search path is `path` and whose home folder
    /// is `home`, starting `catalogue` (or the allow-list, when `None`).
    #[cfg(test)]
    pub(super) fn with_machine(
        catalogue: Option<&'static [RunnableCheck]>,
        path: std::ffi::OsString,
        home: PathBuf,
    ) -> Self {
        Self {
            catalogue,
            machine: Some((path, home)),
            ..Self::new()
        }
    }

    /// Test-only: the same route, starting everything with `env` applied.
    #[cfg(test)]
    pub(super) fn with_env(
        self,
        env: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>,
    ) -> Self {
        Self { env, ..self }
    }

    /// Test-only: the same route, giving rustup `deadline` to answer.
    #[cfg(test)]
    pub(super) fn with_rustup_deadline(self, deadline: Duration) -> Self {
        Self {
            deadline: Some(deadline),
            ..self
        }
    }

    /// A variable of the environment everything this route starts inherits:
    /// this process's own, with the test seam's over it.
    fn var(&self, name: &str) -> Option<std::ffi::OsString> {
        #[cfg(test)]
        if let Some((_, value)) = self.env.iter().rev().find(|(n, _)| n == name) {
            return value.clone();
        }
        std::env::var_os(name)
    }

    /// rustup's home, as rustup started in `root` finds it: `RUSTUP_HOME`, or
    /// `.rustup` in the home folder; a relative one is taken from `root`.
    fn rustup_home(&self, root: &Path) -> Option<PathBuf> {
        let home = self
            .var("RUSTUP_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                self.var("HOME")
                    .filter(|v| !v.is_empty())
                    .map(|h| PathBuf::from(h).join(".rustup"))
            })?;
        Some(if home.is_absolute() {
            home
        } else {
            root.join(home)
        })
    }

    /// How long rustup is given to say which program it would start.
    fn rustup_deadline(&self) -> Duration {
        #[cfg(test)]
        if let Some(deadline) = self.deadline {
            return deadline;
        }
        RUSTUP_DEADLINE
    }

    /// **HQ is stopping (ADV-run-security-5).** Every question still waiting
    /// on rustup is killed with its whole process group, and rustup is never
    /// asked again by this HQ — so no question outlives it, however it was
    /// kept from answering. Each question's own thread reaps its leader; one
    /// that HQ exits before reaching is reaped by the system, having been
    /// killed first.
    pub(super) fn stop_questions(&self) {
        #[cfg(unix)]
        self.stop_questions_with(thruline_core::bounded_child::kill_group_now);
        #[cfg(not(unix))]
        {
            self.questions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .stopping = true;
        }
    }

    /// [`Self::stop_questions`] with its signaller as a seam. Every group
    /// goes through the guarded door (RUN-STOP-PGID-GUARD), which refuses 0,
    /// 1, negative and HQ's own group before any signal; a test passes a
    /// recording fake so no real signal is ever delivered.
    #[cfg(unix)]
    fn stop_questions_with(&self, mut signaller: impl FnMut(i32) -> rustix::io::Result<()>) {
        let mut questions = self
            .questions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        questions.stopping = true;
        for group in &questions.groups {
            // A refusal or failure is not a stop; the leader's own thread
            // reaps it, or the system does once HQ has exited.
            let _ = thruline_core::bounded_child::signal_group(*group, &mut signaller);
        }
    }

    /// The search path HQ itself was started with.
    fn inherited_path(&self) -> std::ffi::OsString {
        #[cfg(test)]
        if let Some((path, _)) = &self.machine {
            return path.clone();
        }
        std::env::var_os("PATH").unwrap_or_default()
    }

    /// The folder one environment adds to the search path, or `None` for HQ's own.
    fn environment_dir(&self, root: &Path, place: Where) -> Option<PathBuf> {
        match place {
            Where::Hq => None,
            Where::Project(rel) => Some(root.join(rel)),
            Where::Home(rel) => {
                #[cfg(test)]
                if let Some((_, home)) = &self.machine {
                    return Some(home.join(rel));
                }
                // `CARGO_HOME` is where cargo itself says its programs live.
                if rel == ".cargo/bin" {
                    if let Some(cargo_home) =
                        std::env::var_os("CARGO_HOME").filter(|v| !v.is_empty())
                    {
                        return Some(PathBuf::from(cargo_home).join("bin"));
                    }
                }
                home().map(|h| h.join(rel))
            }
            Where::Machine(abs) => Some(PathBuf::from(abs)),
        }
    }

    /// The search path a run in this environment gets: its folder first, then
    /// the one HQ was started with, so the program's own helpers are still found.
    fn search_path(&self, root: &Path, place: Where) -> std::ffi::OsString {
        let inherited = self.inherited_path();
        match self.environment_dir(root, place) {
            None => inherited,
            Some(dir) => {
                let mut dirs = vec![dir];
                dirs.extend(std::env::split_paths(&inherited));
                std::env::join_paths(dirs).unwrap_or(inherited)
            }
        }
    }

    pub(super) fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    fn runnable(&self, id: &str) -> Option<&'static RunnableCheck> {
        #[cfg(test)]
        if let Some(catalogue) = self.catalogue {
            return catalogue.iter().find(|c| c.id == id);
        }
        check::runnable_check(id)
    }
}

/// 256 bits from the operating system's random source, as hex. `None` when the
/// source cannot be read — and then every write here is refused, rather than
/// served with a guessable token.
pub(super) fn mint_token() -> Option<String> {
    let mut bytes = [0u8; 32];
    let mut source = std::fs::File::open("/dev/urandom").ok()?;
    source.read_exact(&mut bytes).ok()?;
    Some(hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Equality that does not stop at the first differing byte.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Is this path one of the run route's?
pub(super) fn owns(path: &str) -> bool {
    if path == "/api/run/token" {
        return true;
    }
    let Some(rest) = path.strip_prefix("/api/changes/") else {
        return false;
    };
    match rest.split_once('/') {
        Some((id, tail)) => {
            !id.is_empty()
                && (tail == "run" || tail == "run/permission" || tail == "run/permission/revoke")
        }
        None => false,
    }
}

fn json(stream: &mut TcpStream, status: u16, value: &serde_json::Value) -> Result<()> {
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

fn presented_path_refusal(
    stream: &mut TcpStream,
    ledger: &thruline_core::ledger::Ledger,
    change_id: &str,
    at: &Path,
    value: &serde_json::Value,
) -> Result<()> {
    let policy = match super::change_action_policy(ledger, change_id) {
        Ok(Some(policy)) => policy,
        _ => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    match present_path_refusal_value(value, at, &policy) {
        Ok(served) => json(stream, 403, &served),
        Err(_) => refuse(stream, 503, "exclusion_policy_unavailable"),
    }
}

fn present_path_refusal_value(
    value: &serde_json::Value,
    at: &Path,
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<serde_json::Value> {
    // `shown` changes a home path to `~/...`, which cannot be matched as an
    // absolute project path. Filter the original first, then show `~` only
    // when the path stayed visible.
    let absolute = at.to_string_lossy().to_string();
    let mut raw_value = value.clone();
    raw_value["found_at"] = serde_json::Value::String(absolute.clone());
    let raw = serde_json::to_vec(&raw_value)?;
    let served = crate::exclusion_serve::bytes(&raw, policy)?;
    let mut doc: serde_json::Value = serde_json::from_slice(&served)?;
    if doc["found_at"].as_str() == Some(absolute.as_str()) {
        doc["found_at"] = serde_json::Value::String(shown(at));
    }
    Ok(doc)
}

/// **Guard 2, first half.** Exactly one `Origin`, and it is this server.
/// Also HQ's install route's guard (`install_route.rs`), so the two cannot differ.
pub(super) fn origin_is_ours(state: &State, headers: &RequestHeaders) -> bool {
    if headers.origin_count != 1 {
        return false;
    }
    let port = super::port_of(&state.origin);
    headers.origin == format!("http://{}", state.origin)
        || headers.origin == format!("http://localhost:{port}")
}

/// **Guard 2, second half.** Exactly one token header, equal to this launch's.
/// Also HQ's install route's guard (`install_route.rs`), so the two cannot differ.
pub(super) fn token_is_ours(state: &State, headers: &RequestHeaders) -> bool {
    let Some(ours) = state.run_route.token.as_deref() else {
        return false;
    };
    headers.run_tokens.len() == 1 && same_secret(&headers.run_tokens[0], ours)
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
    if path == "/api/run/token" {
        if method != "GET" {
            return refuse(stream, 405, "method_not_allowed");
        }
        return token(state, headers, stream);
    }
    let rest = &path["/api/changes/".len()..];
    let Some((change_id, tail)) = rest.split_once('/') else {
        return refuse(stream, 404, "no_such_route");
    };
    let change_id = change_id.to_string();
    match (method, tail) {
        ("GET", "run") => view(state, &change_id, stream),
        ("POST", "run") | ("POST", "run/permission") | ("POST", "run/permission/revoke") => {
            // Guard 2 — before the body is read, before the store is opened.
            if !origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            if state.run_route.token.is_none() {
                return refuse(stream, 503, "run_token_unavailable");
            }
            if !token_is_ours(state, headers) {
                return refuse(stream, 403, "run_token_required");
            }
            let body = match read_interpret_body(reader, headers) {
                Ok(body) => body,
                Err(_) => return refuse(stream, 400, "malformed_request"),
            };
            match tail {
                "run" => start(state, &change_id, &body, stream),
                "run/permission" => grant(state, &change_id, &body, stream),
                _ => revoke(state, &change_id, &body, stream),
            }
        }
        _ => refuse(stream, 405, "method_not_allowed"),
    }
}

/// `GET /api/run/token` — this launch's token, to this page and nobody else.
///
/// A browser marks every request with where it came from. A foreign page's
/// request says `cross-site` (or `same-site` for a sibling port) and is
/// refused; this page's own fetch says `same-origin`. A request with no mark
/// at all is a program on this machine, which could read the token out of
/// memory anyway, and the global `Host`/`Origin` check has already refused
/// every foreign `Origin`. The answer carries no CORS header, so even a
/// request that got through could not be read by a foreign page.
fn token(state: &State, headers: &RequestHeaders, stream: &mut TcpStream) -> Result<()> {
    if let Some(site) = headers.sec_fetch_site.as_deref() {
        if site != "same-origin" && site != "none" {
            return refuse(stream, 403, "same_origin_required");
        }
    }
    if headers.origin_count > 0 && !origin_is_ours(state, headers) {
        return refuse(stream, 403, "same_origin_required");
    }
    match state.run_route.token.as_deref() {
        Some(token) => json(stream, 200, &serde_json::json!({ "token": token })),
        None => refuse(stream, 503, "run_token_unavailable"),
    }
}

/* ── the project folder, the permission, the environments ──────────────── */

/// The folder a run happens in: the Change's recorded worktree, the same one
/// a decision record is written under. Never a folder a request named.
fn project_root(
    ledger: &thruline_core::ledger::Ledger,
    change_id: &str,
) -> Result<Option<PathBuf>> {
    Ok(
        thruline_core::change_decision::worktree_root(ledger, change_id)?
            .filter(|root| root.is_dir()),
    )
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// A path as Sam reads it: the home folder as `~`, and every credential shape
/// masked before the text is served.
fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    let text = match home() {
        Some(home) => {
            let home = home.display().to_string();
            match text.strip_prefix(&home) {
                Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
                _ => text,
            }
        }
        None => text,
    };
    masked(&text)
}

/// A stable identity for an exact recorded folder. It is a lookup key, never
/// authority to choose a filesystem path in a write request.
pub(super) fn folder_id(folder: &str) -> String {
    thruline_core::chain::sha256_hex(format!("run-permission-folder-v1\0{folder}").as_bytes())
}

/// The same read-only, no-follow root check a permission act makes before it
/// can write. Settings uses it to avoid offering a button for a replaced link.
pub(super) fn permission_root_usable(root: &Path, missing_ok: bool) -> bool {
    place::pin_for_revoke(root, missing_ok).is_ok()
}

/// A Settings Allow again button also needs a plain cache filename. This is
/// read-only; the write route repeats the check with a pinned directory.
pub(super) fn permission_grant_ready(root: &Path) -> bool {
    place::inspect_for_grant(root).is_ok()
}

fn masked(text: &str) -> String {
    thruline_core::redact::mask(text).0
}

#[derive(Debug, Clone, serde::Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct PermissionFile {
    schema_version: u32,
    project_folder: String,
    granted_by: String,
    granted_at: String,
    /// P6-5B — the `run_permissions` row this file is a cache of.
    grant_id: String,
    promise: String,
    seal: String,
}

/// The permission file's shape. 2 since P6-5B: it names its grant row. A
/// version-1 file names none, so it is a cache of nothing and is not honoured.
const PERMISSION_FILE_VERSION: u32 = 2;

fn key_path(db: &Path) -> PathBuf {
    db.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(KEY_FILE)
}

/// Read the seal key, or — only when `create` — make one.
fn seal_key(db: &Path, create: bool) -> Result<Option<Vec<u8>>> {
    let path = key_path(db);
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() == 32 => return Ok(Some(bytes)),
        Ok(_) => anyhow::bail!("{} is not a seal key", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    }
    if !create {
        return Ok(None);
    }
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .context("reading the operating system's random source")?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            use std::io::Write as _;
            file.write_all(&bytes)
                .with_context(|| format!("writing {}", path.display()))?;
            Ok(Some(bytes.to_vec()))
        }
        // Another request made it first; read theirs.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => seal_key(db, false),
        Err(e) => Err(e).with_context(|| format!("creating {}", path.display())),
    }
}

fn seal(key: &[u8], folder: &str, by: &str, at: &str, grant_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"thruline-run-permission-v2\0");
    h.update(key);
    for part in [folder, by, at, grant_id] {
        h.update([0u8]);
        h.update(part.as_bytes());
    }
    hex(&h.finalize())
}

/// [`PERMISSION_FILE`]'s two names: the folder under the project root, and
/// the file in it.
fn permission_names() -> (&'static str, &'static str) {
    PERMISSION_FILE
        .split_once('/')
        .unwrap_or((".thruline", "run-permission.json"))
}

/// Why the permission's place under the project cannot be used.
#[derive(Debug)]
enum Place {
    /// `.thruline`, or the permission file's own name, is a link or is not
    /// the plain folder or plain file it should be. Nothing is read, written
    /// or removed through it.
    NotPlain,
    /// Anything else the file system said.
    Io(std::io::Error),
}

/// **ADV-run-security-1 (run lane F1).** Every access to the permission file
/// — read, write, removal — is made inside a `.thruline` folder opened
/// without following a link, and the file's own name is opened without
/// following one either. Whoever controls the repository controls what is
/// in that folder; a link planted there must never carry Allow's write, the
/// sheet's read or Take back's removal to a file outside the project.
#[cfg(unix)]
mod place {
    use super::{permission_names, Place};
    use rustix::fd::OwnedFd;
    use rustix::fs::{
        mkdirat, open, openat, renameat, statat, unlinkat, AtFlags, FileType, Mode, OFlags,
    };
    use rustix::io::Errno;
    use std::io::{Read as _, Write as _};
    use std::path::{Component, Path};

    /// The `.thruline` folder, opened.
    pub(super) type Dir = OwnedFd;

    /// Keep the recorded root (and its `.thruline`, when present) pinned from
    /// before the revocation row until its cache has been removed. A path
    /// replaced during the act cannot redirect the removal elsewhere.
    pub(super) struct PinnedRemoval {
        root: Option<OwnedFd>,
        dir: Option<Dir>,
    }

    /// Guarded grant preflight keeps the exact opened directory alive across
    /// the ledger append. A replacement plain directory at the same pathname
    /// cannot become the cache destination after the person's row is checked.
    pub(super) struct PinnedGrant {
        root: OwnedFd,
        dir: Option<Dir>,
    }

    /// No permission file is anywhere near this size.
    const LIMIT: u64 = 64 * 1024;

    fn refused(e: Errno) -> Place {
        // A link where `O_NOFOLLOW` was asked (`ELOOP`; `EMLINK` on some
        // systems), or a file where a folder was asked (`ENOTDIR`).
        if e == Errno::LOOP || e == Errno::MLINK || e == Errno::NOTDIR {
            Place::NotPlain
        } else {
            Place::Io(e.into())
        }
    }

    fn open_root(root: &Path, missing_ok: bool) -> Result<Option<OwnedFd>, Place> {
        if !root.is_absolute() {
            return Err(Place::NotPlain);
        }
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut dir = open(Path::new("/"), flags, Mode::empty()).map_err(refused)?;
        // NOFOLLOW on the whole pathname protects only its final component.
        // Walk every component relative to the prior pinned descriptor so a
        // replacement symlink in any ancestor is refused before the row.
        for component in root.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(name) => {
                    dir = match openat(&dir, name, flags, Mode::empty()) {
                        Ok(next) => next,
                        Err(e) if e == Errno::NOENT && missing_ok => return Ok(None),
                        Err(e) if e == Errno::NOENT => return Err(Place::NotPlain),
                        Err(e) => return Err(refused(e)),
                    };
                }
                Component::ParentDir | Component::Prefix(_) => return Err(Place::NotPlain),
            }
        }
        Ok(Some(dir))
    }

    /// `.thruline` under a pinned recorded root, never through a link.
    /// With `create`, it is made when absent; without, absent is `Ok(None)`.
    fn folder_at(root: &OwnedFd, create: bool) -> Result<Option<Dir>, Place> {
        let (name, _) = permission_names();
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        match openat(root, name, flags, Mode::empty()) {
            Ok(dir) => Ok(Some(dir)),
            Err(e) if e == Errno::NOENT && create => {
                match mkdirat(root, name, Mode::from_raw_mode(0o777)) {
                    Ok(()) => {}
                    Err(e) if e == Errno::EXIST => {}
                    Err(e) => return Err(Place::Io(e.into())),
                }
                openat(root, name, flags, Mode::empty())
                    .map(Some)
                    .map_err(refused)
            }
            Err(e) if e == Errno::NOENT => Ok(None),
            Err(e) => Err(refused(e)),
        }
    }

    fn folder(root: &Path, create: bool) -> Result<Option<Dir>, Place> {
        let Some(root) = open_root(root, !create)? else {
            return Ok(None);
        };
        folder_at(&root, create)
    }

    /// The permission file's bytes, when it is a plain file in a plain
    /// `.thruline`. A link at either name, anything that is not a plain file,
    /// or a file larger than any permission reads as no file at all.
    pub(super) fn read(root: &Path) -> Option<Vec<u8>> {
        let (_, name) = permission_names();
        let dir = folder(root, false).ok()??;
        let fd = openat(
            &dir,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .ok()?;
        let file = std::fs::File::from(fd);
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes).ok()?;
        (bytes.len() as u64 <= LIMIT).then_some(bytes)
    }

    /// `.thruline`, made when absent, ready to take the permission file: the
    /// folder is a plain folder, and the file's name is free or a plain file.
    pub(super) fn prepare(root: &Path) -> Result<Dir, Place> {
        let dir = folder(root, true)?.ok_or(Place::NotPlain)?;
        plain_or_absent(&dir)?;
        Ok(dir)
    }

    fn plain_or_absent(dir: &Dir) -> Result<(), Place> {
        let (_, name) = permission_names();
        match statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) if FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile => Ok(()),
            Ok(_) => Err(Place::NotPlain),
            Err(e) if e == Errno::NOENT => Ok(()),
            Err(e) => Err(Place::Io(e.into())),
        }
    }

    /// Read-only preflight for a Settings grant. A stale row refusal must not
    /// leave an empty `.thruline` behind; creation happens only after append.
    pub(super) fn inspect_for_grant(root: &Path) -> Result<PinnedGrant, Place> {
        let root = open_root(root, false)?.ok_or(Place::NotPlain)?;
        let dir = folder_at(&root, false)?;
        if let Some(dir) = dir.as_ref() {
            plain_or_absent(dir)?;
        }
        Ok(PinnedGrant { root, dir })
    }

    /// Finish a guarded grant through the root/directory inspected before
    /// append. Any plain replacement at the pathname is left untouched.
    pub(super) fn prepare_pinned(pinned: PinnedGrant) -> Result<Dir, Place> {
        let PinnedGrant { root, dir } = pinned;
        let dir = match dir {
            Some(dir) => dir,
            None => folder_at(&root, true)?.ok_or(Place::NotPlain)?,
        };
        plain_or_absent(&dir)?;
        Ok(dir)
    }

    /// A file named `name` in `dir`, made only when the name is free.
    /// `Ok(false)`: something (a file, a link) is already there, left alone.
    pub(super) fn create_new(dir: &Dir, name: &str, bytes: &[u8]) -> std::io::Result<bool> {
        match openat(
            dir,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o666),
        ) {
            Ok(fd) => std::fs::File::from(fd).write_all(bytes).map(|()| true),
            Err(e) if e == Errno::EXIST => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// The permission file, written whole: a new file beside it, then renamed
    /// over the name. A rename replaces whatever is at the name — a link
    /// included — and never follows it; a reader sees the old file or the new
    /// one, never part of either.
    pub(super) fn write(dir: &Dir, bytes: &[u8]) -> std::io::Result<()> {
        let (_, name) = permission_names();
        let temporary = format!(".{name}.{}.tmp", ulid::Ulid::generate());
        let fd = openat(
            dir,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o666),
        )?;
        let mut file = std::fs::File::from(fd);
        let written = file
            .write_all(bytes)
            .and_then(|()| file.sync_all())
            .and_then(|()| renameat(dir, temporary.as_str(), dir, name).map_err(Into::into));
        if written.is_err() {
            // Said by the caller; the half-written file is not left behind.
            let _ = unlinkat(dir, temporary.as_str(), AtFlags::empty());
        }
        written
    }

    /// Pin the root and existing `.thruline` before a revocation row is
    /// appended. A removed historical root is allowed: it has no cache.
    pub(super) fn pin_for_revoke(root: &Path, missing_ok: bool) -> Result<PinnedRemoval, Place> {
        let root = open_root(root, missing_ok)?;
        let dir = match root.as_ref() {
            Some(root) => folder_at(root, false)?,
            None => None,
        };
        Ok(PinnedRemoval { root, dir })
    }

    /// Legacy `{}` Take back could append even if `.thruline` had become a
    /// link; the record then blocks runs, while removal never follows it.
    /// Still pin and validate every root component before the append.
    pub(super) fn pin_root_for_legacy_revoke(root: &Path) -> Result<PinnedRemoval, Place> {
        let root = open_root(root, false)?;
        Ok(PinnedRemoval { root, dir: None })
    }

    /// Remove from the pinned directory, never by reopening the recorded
    /// path after the append. A link at the file's name is unlinked as a link.
    pub(super) fn remove_pinned(pinned: &PinnedRemoval) -> Result<(), Place> {
        let (_, name) = permission_names();
        let later;
        let dir = if let Some(dir) = pinned.dir.as_ref() {
            dir
        } else if let Some(root) = pinned.root.as_ref() {
            later = folder_at(root, false)?;
            let Some(dir) = later.as_ref() else {
                return Ok(());
            };
            dir
        } else {
            return Ok(());
        };
        match unlinkat(dir, name, AtFlags::empty()) {
            Ok(()) => Ok(()),
            Err(e) if e == Errno::NOENT => Ok(()),
            Err(e) => Err(Place::Io(e.into())),
        }
    }
}

/// Without a reader that opens a folder without following a link, the
/// permission is never read, written or removed here, so no check runs from
/// the screen on this platform.
#[cfg(not(unix))]
mod place {
    use super::Place;
    use std::path::Path;

    pub(super) struct Dir;
    pub(super) struct PinnedRemoval;
    pub(super) struct PinnedGrant;

    pub(super) fn read(_root: &Path) -> Option<Vec<u8>> {
        None
    }

    pub(super) fn prepare(_root: &Path) -> Result<Dir, Place> {
        Err(Place::Io(std::io::Error::other(
            "the run permission is only written on unix",
        )))
    }

    pub(super) fn inspect_for_grant(_root: &Path) -> Result<PinnedGrant, Place> {
        Err(Place::Io(std::io::Error::other(
            "the run permission is only written on unix",
        )))
    }

    pub(super) fn prepare_pinned(_pinned: PinnedGrant) -> Result<Dir, Place> {
        Err(Place::Io(std::io::Error::other(
            "the run permission is only written on unix",
        )))
    }

    pub(super) fn create_new(_dir: &Dir, _name: &str, _bytes: &[u8]) -> std::io::Result<bool> {
        Ok(false)
    }

    pub(super) fn write(_dir: &Dir, _bytes: &[u8]) -> std::io::Result<()> {
        Err(std::io::Error::other(
            "the run permission is only written on unix",
        ))
    }

    pub(super) fn pin_for_revoke(_root: &Path, _missing_ok: bool) -> Result<PinnedRemoval, Place> {
        Ok(PinnedRemoval)
    }

    pub(super) fn pin_root_for_legacy_revoke(_root: &Path) -> Result<PinnedRemoval, Place> {
        Ok(PinnedRemoval)
    }

    pub(super) fn remove_pinned(_pinned: &PinnedRemoval) -> Result<(), Place> {
        Ok(())
    }
}

/// **Guard 5.** The permission for this recorded folder, if Sam gave one: the file is
/// present, parses whole, names THIS folder, and carries a seal only the key
/// beside the ledger could have made. It is read without following a link
/// (ADV-run-security-1): a link under the project is not a permission.
pub(super) fn permission(db: &Path, root: &Path) -> Option<PermissionFile> {
    let raw = place::read(root)?;
    let file: PermissionFile = serde_json::from_slice(&raw).ok()?;
    let key = seal_key(db, false).ok()??;
    let folder = root.display().to_string();
    if file.schema_version != PERMISSION_FILE_VERSION || file.project_folder != folder {
        return None;
    }
    let expected = seal(
        &key,
        &file.project_folder,
        &file.granted_by,
        &file.granted_at,
        &file.grant_id,
    );
    same_secret(&expected, &file.seal).then_some(file)
}

/// Test-only: write a correctly sealed permission file naming `grant_id`,
/// the way a file this machine's key sealed but the record never granted
/// would look (a store restored from elsewhere, a file written before P6-5B).
#[cfg(test)]
pub(super) fn write_sealed_file_for_test(
    db: &Path,
    root: &Path,
    by: &str,
    at: &str,
    grant_id: &str,
) -> Result<()> {
    let key = seal_key(db, true)?.context("the seal key was not created")?;
    let folder = root.display().to_string();
    let file = PermissionFile {
        schema_version: PERMISSION_FILE_VERSION,
        seal: seal(&key, &folder, by, at, grant_id),
        project_folder: folder,
        granted_by: by.to_string(),
        granted_at: at.to_string(),
        grant_id: grant_id.to_string(),
        promise: PROMISE.to_string(),
    };
    std::fs::create_dir_all(root.join(".thruline"))?;
    std::fs::write(
        root.join(PERMISSION_FILE),
        serde_json::to_vec_pretty(&file)?,
    )?;
    Ok(())
}

/// P6-5B — where this project's permission stands, the record and its cache
/// read together. Only [`ConsentState::Given`] lets a check run.
#[derive(Debug, Clone)]
pub(super) enum ConsentState {
    /// The latest row for this folder is a grant, and the file is its cache.
    Given {
        grant: Box<thruline_core::run_permission::ConsentRow>,
    },
    /// Nothing in the record and no sealed file: never asked.
    Never,
    /// The latest row took the grant back. Refused whatever the file says.
    Revoked,
    /// A sealed file names no standing grant in the record.
    NotInRecord,
    /// The record holds a standing grant, but no valid cache of it is in the
    /// folder (gone, or not sealed by this machine), so Sam is asked again.
    FileMissing,
}

impl ConsentState {
    /// The word the sheet and a refusal carry.
    fn word(&self) -> &'static str {
        match self {
            Self::Given { .. } => "given",
            Self::Never => "never",
            Self::Revoked => "revoked",
            Self::NotInRecord => "not_in_record",
            Self::FileMissing => "file_missing",
        }
    }

    /// Why a run is refused in this state; `None` when it may go.
    fn refusal(&self) -> Option<&'static str> {
        match self {
            Self::Given { .. } => None,
            Self::Never => Some("permission_required"),
            Self::Revoked => Some("permission_revoked"),
            Self::NotInRecord => Some("permission_not_in_record"),
            // P6-5's refusal, unchanged: there is no valid permission here.
            // The sheet's `consent.state` says why in words.
            Self::FileMissing => Some("permission_required"),
        }
    }
}

/// **Guards 5, 5B-1 and 5B-2.** Read the latest `run_permissions` row for this
/// folder and the sealed file, and say where they stand together. The record
/// wins: the file is honoured only as the cache of the standing grant.
pub(super) fn consent(
    ledger: &thruline_core::ledger::Ledger,
    db: &Path,
    root: &Path,
) -> Result<ConsentState> {
    Ok(consent_and_latest(ledger, db, root)?.0)
}

/// [`consent`], with the id of the folder's latest `run_permissions` row the
/// answer rests on (`None`: nothing recorded yet) — what a grant is appended
/// over, so a press whose view of the record has moved appends nothing
/// (ADV-run-ledger-4).
fn consent_and_latest(
    ledger: &thruline_core::ledger::Ledger,
    db: &Path,
    root: &Path,
) -> Result<(ConsentState, Option<String>)> {
    use thruline_core::run_permission as rp;
    let folder = root.display().to_string();
    let standing = rp::standing(ledger, &folder)?;
    let latest = rp::latest_id(&standing).map(str::to_string);
    Ok((consent_for_standing(db, root, standing), latest))
}

/// Use one recorded standing for the effective state and its response guard.
/// Settings already selected the standing while collecting this HQ's folders;
/// reading it again could mix a newer state with an older latest-row id.
pub(super) fn consent_for_standing(
    db: &Path,
    root: &Path,
    standing: thruline_core::run_permission::Standing,
) -> ConsentState {
    use thruline_core::run_permission::Standing;
    let file = permission(db, root);
    match standing {
        Standing::Revoked(_) => ConsentState::Revoked,
        Standing::Granted(grant) => match file {
            Some(f)
                if f.grant_id == grant.id
                    && f.granted_by == grant.actor
                    && f.granted_at == grant.recorded_at =>
            {
                ConsentState::Given {
                    grant: Box::new(grant),
                }
            }
            Some(_) => ConsentState::NotInRecord,
            None => ConsentState::FileMissing,
        },
        Standing::Never => match file {
            Some(_) => ConsentState::NotInRecord,
            None => ConsentState::Never,
        },
    }
}

/// One row of the permission's history as the sheet serves it: who, when,
/// what, and — for the Technical face — where it is in the record.
fn consent_row_json(row: &thruline_core::run_permission::ConsentRow) -> serde_json::Value {
    serde_json::json!({
        "act": row.act.as_str(),
        "actor": masked(&row.actor),
        "at": row.recorded_at,
        "revokes": row.revokes,
        "receipt": {
            "table": thruline_core::run_permission::TABLE,
            "seq": row.seq,
            "id": row.id,
            "chain_hash": row.chain_hash,
        },
    })
}

/// The project a run happens in, as the folders under it are compared: the
/// recorded root, and the same root with every link in it resolved.
struct Project<'a> {
    root: &'a Path,
    canonical: PathBuf,
}

impl<'a> Project<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            canonical: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        }
    }

    /// Is `path` the project root or anything under it? Everything there is
    /// the repository's to write.
    fn holds(&self, path: &Path) -> bool {
        path.starts_with(&self.canonical) || path.starts_with(self.root)
    }

    /// Does `path` lead into the project — as written, with its `..` folded
    /// away, or with every link in it resolved?
    fn leads_into(&self, path: &Path) -> bool {
        self.holds(path)
            || self.holds(&folded(path))
            || path.canonicalize().is_ok_and(|file| self.holds(&file))
    }

    /// The project's folder and every folder above it, as recorded and as
    /// resolved: the folders rustup looks at for a run started here.
    fn folders(&self) -> Vec<&Path> {
        let mut folders: Vec<&Path> = Vec::new();
        for folder in self.root.ancestors().chain(self.canonical.ancestors()) {
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        folders
    }
}

/// `path` with `.` dropped and each `..` taking away the folder before it,
/// without looking at the file system.
fn folded(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// What looking for a check's program on a search path found.
#[derive(Debug)]
enum Found {
    /// A program outside the project, and the search path its run gets:
    /// every folder of the path that exists and is not the project's, each
    /// with its links resolved — and, when the program is rustup's proxy, the
    /// toolchain the guard checked, which the run is pinned to
    /// (`RUSTUP_TOOLCHAIN`, ADV-run-security-7).
    Outside(std::ffi::OsString, Option<std::ffi::OsString>),
    /// Nowhere on the path — or rustup's proxy is, and rustup names no
    /// program it would start without installing a toolchain first.
    Nowhere,
    /// The first program of that name on the path is the project's own — in
    /// a folder under the project root, in a relative folder (the run's
    /// working folder is the project, so it resolves there), or a link to a
    /// file under the root — or it is rustup's proxy and would hand the run to
    /// such a program. Named here so the refusal can say where.
    InProject(PathBuf),
    /// Outside the project, and the guard cannot tell which program a run
    /// would start: a toolchain file rustup would read, above the project, is
    /// not a plain file, or rustup did not answer in time. Nothing runs.
    /// `why` is a closed-set word; `at` is what the guard was looking at.
    Undecided { why: &'static str, at: PathBuf },
}

/// **Guard 4 (ADV-run-security-2, run lane F2).** Where `program` would be
/// found on `path` for a run whose working folder is the project, and the
/// search path that run gets.
///
/// Whoever controls the repository controls every file under the project
/// root, so a program there is not the tool it is named after: a `cargo` in
/// `node_modules/.bin` can print "test result: ok" without running a test.
/// Every folder on the path is canonicalized; a folder inside the project —
/// or a relative one — is never handed on to the run; and a program whose
/// first match is in such a folder, or is a link to a file there, refuses
/// the run before anything starts. What the check itself builds and runs is
/// the project's code, as it always is; the program that reports on it is
/// not.
///
/// A program found outside the project can still hand the run to one inside
/// it: rustup's proxy is asked about below ([`handed_to`]), and the program
/// it would start is held to the same rule.
fn resolve(route: &RunRoute, program: &str, root: &Path, path: &std::ffi::OsStr) -> Found {
    let project = Project::new(root);
    let mut kept: Vec<PathBuf> = Vec::new();
    let mut first: Option<Result<PathBuf, PathBuf>> = None;
    for entry in std::env::split_paths(path) {
        let (folder, in_project) = if entry.is_absolute() {
            match entry.canonicalize() {
                Ok(folder) => {
                    let in_project = project.holds(&folder) || project.holds(&entry);
                    (folder, in_project)
                }
                // Not there now, so nothing in it can be found or run.
                Err(_) => continue,
            }
        } else {
            (root.join(&entry), true)
        };
        let candidate = folder.join(program);
        if first.is_none() && is_executable(&candidate) {
            let into_project = in_project
                || candidate
                    .canonicalize()
                    .map_or(true, |file| project.holds(&file));
            first = Some(if into_project {
                Err(candidate)
            } else {
                Ok(candidate)
            });
        }
        if !in_project && std::env::join_paths([&folder]).is_ok() {
            kept.push(folder);
        }
    }
    match first {
        Some(Err(at)) => Found::InProject(at),
        None => Found::Nowhere,
        Some(Ok(found)) => {
            if let Some(refused) = toolchain_data_refuses(route, program, &project) {
                return refused;
            }
            match std::env::join_paths(&kept) {
                Ok(search_path) => {
                    match handed_to(route, program, &found, &project, &search_path) {
                        Handoff::Direct => Found::Outside(search_path, None),
                        Handoff::Outside(toolchain) => Found::Outside(search_path, Some(toolchain)),
                        Handoff::InProject(at) => Found::InProject(at),
                        Handoff::Unanswered => Found::Nowhere,
                        Handoff::Unsure(why, rustup) => Found::Undecided { why, at: rustup },
                    }
                }
                Err(_) => Found::Nowhere,
            }
        }
    }
}

/// Preparing a Change is no permission to execute a program the project
/// supplied, even to ask its version. Apply the run guard to this earlier
/// observation too, then ask from the selected root with exactly the guarded
/// search path and pinned toolchain. An unsafe or unanswered probe has no
/// version; it never borrows the planted program's answer as a fact.
pub(crate) fn probe_tool_for_prepare(
    program: &str,
    root: &Path,
) -> thruline_core::examined_inputs::ToolInput {
    use thruline_core::examined_inputs::{Input, ToolInput};

    let route = RunRoute::new();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut tool = ToolInput {
        name: program.to_string(),
        version: Input::Unknown,
        present: true,
    };
    let (path, toolchain) = match resolve(&route, program, root, &path) {
        Found::Outside(path, toolchain) => (path, toolchain),
        Found::Nowhere => {
            tool.present = false;
            return tool;
        }
        Found::InProject(_) | Found::Undecided { .. } => return tool,
    };
    let mut ask = std::process::Command::new(program);
    ask.arg("--version")
        .current_dir(root)
        .env("PATH", path)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env("RUSTUP_DIST_SERVER", NO_DIST_SERVER);
    if let Some(toolchain) = toolchain {
        ask.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    if let Asked::Answered(bytes) = ask_bounded(&route, ask) {
        tool.version = Input::known(
            String::from_utf8_lossy(&bytes)
                .lines()
                .next()
                .unwrap_or_default()
                .trim(),
        );
    }
    tool
}

/// The programs rustup installs as its proxies. Each one reads the toolchain
/// in effect for the folder it is started in, from the files below among
/// other places, and starts that toolchain's program of the same name.
const RUSTUP_PROXIES: &[&str] = &[
    "cargo",
    "cargo-clippy",
    "cargo-fmt",
    "cargo-miri",
    "clippy-driver",
    "rls",
    "rust-analyzer",
    "rust-gdb",
    "rust-gdbgui",
    "rust-lldb",
    "rustc",
    "rustdoc",
    "rustfmt",
];

/// The toolchain files rustup reads in each folder, both of them.
const TOOLCHAIN_FILES: [&str; 2] = ["rust-toolchain", "rust-toolchain.toml"];

/// No toolchain file is anywhere near this size.
const TOOLCHAIN_FILE_LIMIT: u64 = 64 * 1024;

/// No rustup `settings.toml` is anywhere near this size.
const SETTINGS_LIMIT: u64 = 1024 * 1024;

/// Every place a toolchain file rustup would read for a run in `project` can
/// be: both names, in the project's folder and in every folder above it up to
/// the root of the file system — for the folder as recorded and as the file
/// system resolves it, since rustup walks up from the resolved one.
fn toolchain_file_places(project: &Project) -> Vec<PathBuf> {
    let mut places = Vec::new();
    for folder in project.folders() {
        for name in TOOLCHAIN_FILES {
            places.push(folder.join(name));
        }
    }
    places
}

/// What is at a place rustup would read.
#[derive(Debug)]
enum Looked {
    /// Nothing.
    Absent,
    /// A plain file within the bound, and every byte of it.
    Bytes(Vec<u8>),
    /// Something rustup must not be sent to read: a FIFO, a link (where links
    /// are not followed), a folder, a device, a file over the bound, a file
    /// that changed between the look and the read, or a place that cannot
    /// even be looked at.
    NotPlain,
}

/// The bytes at `place`, when it is a plain file of at most `limit` bytes.
/// Looked at first without opening it; then opened without waiting on a
/// writer (and, unless `follow`, without following a link), confirmed to be
/// the same plain file, and read to the bound — so a FIFO is never waited on,
/// even one put there between the look and the read.
#[cfg(unix)]
fn plain_bytes(place: &Path, limit: u64, follow: bool) -> Looked {
    use rustix::fs::{open, Mode, OFlags};
    use std::os::unix::fs::MetadataExt;
    let looked = if follow {
        std::fs::metadata(place)
    } else {
        std::fs::symlink_metadata(place)
    };
    let meta = match looked {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Looked::Absent,
        Err(_) => return Looked::NotPlain,
    };
    if !meta.file_type().is_file() || meta.len() > limit {
        return Looked::NotPlain;
    }
    let mut flags = OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC;
    if !follow {
        flags |= OFlags::NOFOLLOW;
    }
    let Ok(fd) = open(place, flags, Mode::empty()) else {
        return Looked::NotPlain;
    };
    let file = std::fs::File::from(fd);
    match file.metadata() {
        Ok(now) if now.is_file() && now.dev() == meta.dev() && now.ino() == meta.ino() => {}
        _ => return Looked::NotPlain,
    }
    let mut bytes = Vec::new();
    match file.take(limit + 1).read_to_end(&mut bytes) {
        Ok(_) if bytes.len() as u64 <= limit => Looked::Bytes(bytes),
        _ => Looked::NotPlain,
    }
}

/// Without an open that cannot wait on a writer, only an absent place is
/// known to be safe; no check runs from the screen on this platform anyway.
#[cfg(not(unix))]
fn plain_bytes(place: &Path, _limit: u64, _follow: bool) -> Looked {
    match std::fs::symlink_metadata(place) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Looked::Absent,
        _ => Looked::NotPlain,
    }
}

/// A TOML document's top-level table, or `None` when it is not one.
fn toml_table(bytes: &[u8]) -> Option<toml::Table> {
    std::str::from_utf8(bytes).ok()?.parse::<toml::Table>().ok()
}

/// The folder a toolchain named `value` lives in, when it is the project's —
/// in it, or linked into it — as the program under it that a run would start,
/// which the refusal names. `value` is a path when absolute, or relative to
/// `base` where rustup reads it as a path (a toolchain file's `path`); any
/// other value is a toolchain name in rustup's home, together with every
/// installed toolchain rustup could expand that name to (`stable` is
/// `stable-<host>`).
fn toolchain_into_project(
    value: &str,
    base: Option<&Path>,
    rustup_home: Option<&Path>,
    program: &str,
    project: &Project,
) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut toolchains = Vec::new();
    let as_path = Path::new(value);
    if as_path.is_absolute() {
        toolchains.push(as_path.to_path_buf());
    } else if let Some(base) = base {
        toolchains.push(base.join(as_path));
    } else if let Some(home) = rustup_home {
        let installed = home.join("toolchains");
        toolchains.push(installed.join(value));
        let family = format!("{}-", value.split('-').next().unwrap_or(value));
        if let Ok(entries) = std::fs::read_dir(&installed) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&family))
                {
                    toolchains.push(entry.path());
                }
            }
        }
    }
    toolchains.into_iter().find_map(|toolchain| {
        let started = toolchain.join("bin").join(program);
        (project.leads_into(&toolchain) || project.leads_into(&started)).then_some(started)
    })
}

/// **Guard 4, what rustup reads (ADV-run-security-5 and -6, D-038 r1).** When
/// the program is one of rustup's proxies, what rustup would read to choose
/// the toolchain is read here first, as data and read-only — whether or not
/// the program found is recognised as the proxy, since a proxy that is a byte
/// copy of rustup is not (ADV-run-security-6):
///
/// - `RUSTUP_HOME` itself, when it is in the project: every toolchain there
///   is the project's;
/// - `RUSTUP_TOOLCHAIN` in the environment HQ was started with;
/// - rustup's `settings.toml`: its default toolchain, and the directory
///   override for the project's folder or any folder above it;
/// - every toolchain file rustup would read ([`toolchain_file_places`]).
///   Each is looked at first without opening it, and anything there that is
///   not a plain file of a toolchain file's size refuses the run
///   (ADV-run-security-5): as `program_in_project`, naming the file, when it
///   is under the project root; otherwise as undecided, naming it. A plain one
///   is read to a bound and understood both ways rustup can: one line naming a
///   toolchain or a path, and TOML with `[toolchain] path` or `channel`.
///
/// A toolchain any of them names that is in the project, or links into it
/// ([`toolchain_into_project`]), refuses the run as `program_in_project`,
/// naming the program under it. Nothing is asked and nothing runs.
fn toolchain_data_refuses(route: &RunRoute, program: &str, project: &Project) -> Option<Found> {
    if !RUSTUP_PROXIES.contains(&program) {
        return None;
    }
    let home = route.rustup_home(project.root);
    let into = |value: &str, base: Option<&Path>| {
        toolchain_into_project(value, base, home.as_deref(), program, project).map(Found::InProject)
    };
    if let Some(home) = &home {
        if project.leads_into(home) {
            return Some(Found::InProject(home.clone()));
        }
    }
    if let Some(value) = route.var("RUSTUP_TOOLCHAIN") {
        if let Some(refused) = value.to_str().and_then(|value| into(value, None)) {
            return Some(refused);
        }
    }
    if let Some(home) = &home {
        let settings = home.join("settings.toml");
        match plain_bytes(&settings, SETTINGS_LIMIT, true) {
            Looked::Absent => {}
            Looked::NotPlain => {
                return Some(Found::Undecided {
                    why: "rustup_settings_not_plain",
                    at: settings,
                })
            }
            Looked::Bytes(bytes) => {
                if let Some(table) = toml_table(&bytes) {
                    let default = table.get("default_toolchain").and_then(|v| v.as_str());
                    if let Some(refused) = default.and_then(|value| into(value, None)) {
                        return Some(refused);
                    }
                    let folders = project.folders();
                    let overrides = table.get("overrides").and_then(|v| v.as_table());
                    for (folder, value) in overrides.into_iter().flatten() {
                        let folder = Path::new(folder);
                        let here = folders
                            .iter()
                            .any(|f| *f == folder || folder.canonicalize().is_ok_and(|c| c == *f));
                        if let Some(refused) = value
                            .as_str()
                            .filter(|_| here)
                            .and_then(|value| into(value, None))
                        {
                            return Some(refused);
                        }
                    }
                }
            }
        }
    }
    for place in toolchain_file_places(project) {
        let bytes = match plain_bytes(&place, TOOLCHAIN_FILE_LIMIT, false) {
            Looked::Absent => continue,
            Looked::Bytes(bytes) => bytes,
            Looked::NotPlain if project.holds(&place) => return Some(Found::InProject(place)),
            Looked::NotPlain => {
                return Some(Found::Undecided {
                    why: "toolchain_file_not_plain",
                    at: place,
                })
            }
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        if text.lines().count() == 1 {
            if let Some(refused) = into(text, None) {
                return Some(refused);
            }
        }
        let section = toml_table(&bytes)
            .and_then(|mut table| table.remove("toolchain"))
            .and_then(|section| section.as_table().cloned());
        if let Some(section) = section {
            let path = section.get("path").and_then(|v| v.as_str());
            if let Some(refused) = path.and_then(|value| into(value, place.parent())) {
                return Some(refused);
            }
            let channel = section.get("channel").and_then(|v| v.as_str());
            if let Some(refused) = channel.and_then(|value| into(value, None)) {
                return Some(refused);
            }
        }
    }
    None
}

/// The dist server rustup is given when it is only asked a question: a local
/// file address that names nothing, so no download it attempts can start.
const NO_DIST_SERVER: &str = "file:///thruline-asks-and-never-downloads";

/// Where a program found outside the project hands the run on to.
#[derive(Debug)]
enum Handoff {
    /// Not rustup's proxy: the program found is the program that runs.
    Direct,
    /// rustup's proxy, and the program rustup names is outside the project:
    /// the toolchain it is in, as `RUSTUP_TOOLCHAIN` names it, which the run
    /// is pinned to.
    Outside(std::ffi::OsString),
    /// rustup's proxy, and the program rustup names is the project's — under
    /// the root, or a link to a file there. Named, so the refusal can say where.
    InProject(PathBuf),
    /// rustup's proxy, and rustup names no program it would start without
    /// installing a toolchain first.
    Unanswered,
    /// rustup's proxy, and the guard cannot tell: rustup (the one named) did
    /// not answer before [`RUSTUP_DEADLINE`], HQ is stopping, or its answer
    /// is not a program in a toolchain's `bin` folder. A closed-set word says
    /// which.
    Unsure(&'static str, PathBuf),
}

/// How long rustup is given to say which program it would start. It answers
/// in milliseconds; a question still going after this is waiting on something
/// it should not be, and is ended.
const RUSTUP_DEADLINE: Duration = Duration::from_secs(4);

/// How often a question to rustup is looked at while it is going.
const QUESTION_POLL: Duration = Duration::from_millis(10);

/// No answer rustup gives to `which` is anywhere near this long.
const ANSWER_LIMIT: u64 = 64 * 1024;

/// What became of one question to rustup.
#[derive(Debug)]
enum Asked {
    /// It ended by itself with success, and this is what it wrote.
    Answered(Vec<u8>),
    /// It ended by itself and said no, or could not be started.
    Declined,
    /// It did not end before the deadline, it was ended by HQ stopping or by a
    /// signal, it could not be watched, or HQ is stopping and it was not asked.
    Unsure,
}

/// Put one question to rustup, bounded (ADV-run-security-5). The question runs
/// in a process group of its own. It is watched without being reaped until it
/// ends or [`RUSTUP_DEADLINE`] passes; then — whichever happened — its whole
/// group is killed and its leader reaped, both under the question table's
/// lock, so nothing the question started survives it and HQ's stop never
/// signals a group whose leader is already gone. Its answer is what it wrote
/// by then, read without waiting.
#[cfg(unix)]
fn ask_bounded(route: &RunRoute, mut ask: std::process::Command) -> Asked {
    use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
    use std::os::unix::process::{CommandExt, ExitStatusExt};

    ask.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    let deadline = Instant::now() + route.rustup_deadline();
    let (mut child, pid) = {
        let Ok(mut questions) = route.questions.lock() else {
            return Asked::Unsure;
        };
        if questions.stopping {
            return Asked::Unsure;
        }
        let Ok(child) = ask.spawn() else {
            return Asked::Declined;
        };
        let pid = Pid::from_child(&child);
        questions.groups.insert(pid.as_raw_nonzero().get());
        (child, pid)
    };
    let ended = loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(Some(_)) => break true,
            Ok(None) => {}
            Err(e) if e == rustix::io::Errno::INTR => {}
            Err(_) => break false,
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(QUESTION_POLL);
    };
    let status = {
        let mut questions = route
            .questions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Through the guarded door (RUN-STOP-PGID-GUARD): the group is this
        // question's own, and the guard refuses anything that is not one.
        let _ = thruline_core::bounded_child::signal_group(
            pid.as_raw_nonzero().get(),
            thruline_core::bounded_child::kill_group_now,
        );
        let status = child.wait();
        questions.groups.remove(&pid.as_raw_nonzero().get());
        status
    };
    match status {
        Ok(status) if ended && status.signal().is_none() => {
            if !status.success() {
                return Asked::Declined;
            }
            match child.stdout.take() {
                Some(out) => Asked::Answered(written_by_now(out)),
                None => Asked::Unsure,
            }
        }
        _ => Asked::Unsure,
    }
}

/// Without a process group of its own to kill, a question to rustup cannot be
/// bounded, so it is not asked; no check runs from the screen on this platform
/// in any case (the permission is never written here).
#[cfg(not(unix))]
fn ask_bounded(_route: &RunRoute, _ask: std::process::Command) -> Asked {
    Asked::Unsure
}

/// What a question wrote before it ended, read without waiting on anything
/// that might still hold the pipe open.
#[cfg(unix)]
fn written_by_now(out: std::process::ChildStdout) -> Vec<u8> {
    let mut bytes = Vec::new();
    if rustix::fs::fcntl_setfl(&out, rustix::fs::OFlags::NONBLOCK).is_ok() {
        let _ = out.take(ANSWER_LIMIT).read_to_end(&mut bytes);
    }
    bytes
}

/// **Guard 4, the toolchain (ADV-run-security-3, D-038 pre-gate).** rustup's
/// proxy is not the program that runs. `~/.cargo/bin/cargo` reads the
/// toolchain in effect for the folder it is started in — `RUSTUP_TOOLCHAIN`, a
/// directory override, a `rust-toolchain.toml` or `rust-toolchain` file in the
/// project or above it — and hands off to that toolchain's `cargo`, and a
/// `path` toolchain can be a folder the repository wrote. So when the program
/// found is rustup's proxy, the rustup it is (the same file, under its own
/// name) is asked `rustup which <program>` in the project's folder, with the
/// run's search path and this process's environment — the same folder and
/// environment the run gets — and the answer is held to guard 4's rule.
/// rustup reads the project's files as data to answer; nothing of the
/// project's is started to ask, and asking never installs. A pinned toolchain
/// outside the project is not overridden: the run goes as it always did, and
/// rustup starts that toolchain. The question is bounded ([`ask_bounded`]):
/// a rustup that has not answered by [`RUSTUP_DEADLINE`] is ended, and the
/// guard says it cannot tell ([`Handoff::Unsure`]) — nothing runs.
fn handed_to(
    route: &RunRoute,
    program: &str,
    found: &Path,
    project: &Project,
    search_path: &std::ffi::OsStr,
) -> Handoff {
    let Some(rustup) = rustup_that_is(found, search_path) else {
        return Handoff::Direct;
    };
    let mut ask = std::process::Command::new(&rustup);
    ask.args(["which", program]).current_dir(project.root);
    #[cfg(test)]
    for (name, value) in &route.env {
        match value {
            Some(value) => ask.env(name, value),
            None => ask.env_remove(name),
        };
    }
    #[cfg(not(test))]
    let _ = route;
    // After the test seam, so nothing can take these back: the run's search
    // path, and a question that never installs a toolchain to answer itself —
    // `RUSTUP_AUTO_INSTALL=0` for a rustup that reads it (1.29.1 does), and
    // for one that tries to install anyway, a dist server that is a local
    // name for nothing, so it fails without a byte on the network. Neither
    // changes which installed toolchain rustup names.
    ask.env("PATH", search_path)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env("RUSTUP_DIST_SERVER", NO_DIST_SERVER);
    let answer = match ask_bounded(route, ask) {
        Asked::Unsure => return Handoff::Unsure("rustup_did_not_answer", rustup),
        Asked::Declined => None,
        Asked::Answered(written) => String::from_utf8(written)
            .ok()
            .and_then(|text| text.lines().next().map(|line| PathBuf::from(line.trim())))
            .filter(|path| path.is_absolute()),
    };
    let Some(answer) = answer else {
        return Handoff::Unanswered;
    };
    match answer.canonicalize() {
        Ok(file) if project.holds(&file) || project.holds(&answer) => Handoff::InProject(answer),
        Ok(_) => match toolchain_of(&answer, program, route.rustup_home(project.root).as_deref()) {
            Some(toolchain) => Handoff::Outside(toolchain),
            None => Handoff::Unsure("rustup_answer_not_a_toolchain", rustup),
        },
        // rustup names a program that is not there: nothing would start.
        Err(_) => Handoff::Unanswered,
    }
}

/// **The toolchain the guard checked (ADV-run-security-7).** rustup answers
/// `which` with `<toolchain>/bin/<program>`; this is that toolchain as
/// `RUSTUP_TOOLCHAIN` names it — by name when it is installed in rustup's
/// home, as rustup names it to the programs it starts, and by its folder
/// otherwise. `None` when the answer does not have that shape, and then the
/// guard cannot pin the run, so it does not start it.
fn toolchain_of(
    answer: &Path,
    program: &str,
    rustup_home: Option<&Path>,
) -> Option<std::ffi::OsString> {
    let bin = answer
        .parent()
        .filter(|_| answer.file_name() == Some(std::ffi::OsStr::new(program)))?;
    let toolchain = bin
        .parent()
        .filter(|_| bin.file_name() == Some(std::ffi::OsStr::new("bin")))?;
    let installed = rustup_home.map(|home| home.join("toolchains"));
    match (toolchain.parent(), toolchain.file_name()) {
        (Some(parent), Some(name)) if installed.as_deref() == Some(parent) => {
            Some(name.to_os_string())
        }
        _ => Some(toolchain.as_os_str().to_os_string()),
    }
}

/// The `rustup` that `found` is — the same file, reached under the name that
/// makes it answer as rustup — when `found` is one of rustup's proxies. Looked
/// for beside `found`, beside the file it links to, and on the run's search
/// path, all of which are outside the project by the time this is asked.
fn rustup_that_is(found: &Path, search_path: &std::ffi::OsStr) -> Option<PathBuf> {
    let target = found.canonicalize().ok()?;
    found
        .parent()
        .into_iter()
        .chain(target.parent())
        .map(Path::to_path_buf)
        .chain(std::env::split_paths(search_path))
        .map(|folder| folder.join("rustup"))
        .find(|rustup| is_executable(rustup) && same_file(rustup, &target))
}

/// Are `a` and `b` one file — a link, a hard link, or the same name?
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
    }
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn environment(id: &str) -> Option<(&'static str, Where)> {
    ENVIRONMENTS
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(known, place)| (*known, *place))
}

fn place_kind(place: Where) -> &'static str {
    match place {
        Where::Hq => "hq",
        Where::Project(_) => "project",
        Where::Home(_) | Where::Machine(_) => "machine",
    }
}

/// Every environment that exists, with which of `programs` it has. HQ's own is
/// always listed, first, because it is where a run goes by default. A folder
/// inside the project is never offered (ADV-run-security-2): the repository
/// writes what is in it, so a run is never started from it.
fn environments_found(
    route: &RunRoute,
    root: &Path,
    programs: &[&'static str],
) -> Vec<serde_json::Value> {
    let project = Project::new(root);
    let mut out = Vec::new();
    for (id, place) in ENVIRONMENTS {
        let dir = route.environment_dir(root, *place);
        if let Some(dir) = &dir {
            if !dir.is_dir() {
                continue;
            }
            let canonical = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            if project.holds(dir) || project.holds(&canonical) {
                continue;
            }
        }
        let path = route.search_path(root, *place);
        let has: Vec<&str> = programs
            .iter()
            .copied()
            .filter(|program| {
                // A folder "has" a program only when the program is IN it —
                // not when the inherited path behind it happens to. Asked
                // first, so rustup is only ever asked about a folder that has
                // the program.
                let here = match &dir {
                    Some(dir) => is_executable(&dir.join(program)),
                    None => true,
                };
                // Only a program a run here would actually start.
                here && matches!(resolve(route, program, root, &path), Found::Outside(..))
            })
            .collect();
        out.push(serde_json::json!({
            "id": id,
            "kind": place_kind(*place),
            "folder": dir.as_deref().map(shown),
            "has": has,
        }));
    }
    out
}

/* ── the outcome, from typed record fields only ────────────────────────── */

/// The words the check's own COMMAND_STARTED receipt carries when ThruLine's
/// own fetch step did not run ([`check::FETCH_SKIPPED_FIELD`]), masked as the
/// failure line is. `None` when the fetch ran, or when the command had none.
fn fetch_skipped_of(
    ledger: &thruline_core::ledger::Ledger,
    run_key: &str,
) -> Result<Option<String>> {
    use thruline_core::observation::{self, ObservationType};
    let mut said = None;
    for row in observation::for_session(ledger, run_key)? {
        if row.source_adapter != "thruline"
            || row.observation_type() != Some(ObservationType::CommandStarted)
        {
            continue;
        }
        if let Some(words) = row
            .receipt()?
            .extra
            .get(check::FETCH_SKIPPED_FIELD)
            .and_then(|value| value.as_str())
        {
            said = Some(masked(words));
        }
    }
    Ok(said)
}

/// The six outcomes, and "still going". Derived from the execution record,
/// the binding's exit status and stream captures, and the typed test count —
/// the same fields `change_evidence::observed_of` reads for a test run, so the
/// sheet and the coverage layer cannot disagree about what a run settled.
fn outcome_of(
    ledger: &thruline_core::ledger::Ledger,
    record: &thruline_core::run_execution::ExecutionRecord,
) -> Result<serde_json::Value> {
    use thruline_core::evidence_binding::StreamCapture;
    use thruline_core::run_execution::{ExecutionState as S, Reason as R};

    let mut exit_code = None;
    let mut signal = None;
    let mut tally = None;
    let mut failure_line = None;
    let mut streams_whole = false;
    if let Some(binding_id) = &record.binding_id {
        if let Some(stored) = thruline_core::evidence_binding::load(ledger, binding_id)? {
            exit_code = stored.binding.exit_code;
            signal = stored.binding.term_signal;
            streams_whole = matches!(stored.binding.stdout, StreamCapture::Captured { .. })
                && matches!(stored.binding.stderr, StreamCapture::Captured { .. });
        }
        tally = thruline_core::test_tally::of_binding(ledger, binding_id)?;
        failure_line = thruline_core::run_record::of_binding(ledger, binding_id)?
            .and_then(|r| r.failure_line)
            .map(|line| masked(&line));
    }
    // OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4): whether ThruLine's own fetch
    // step was skipped for this run, and why, as the run's own record says it.
    let fetch_skipped = fetch_skipped_of(ledger, &record.run_key)?;
    // C30 R-RUN: how long it has run, from its own recorded start.
    let started = recorded_start(ledger, record)?;

    let (outcome, why) = match (record.state, record.reason) {
        (S::Running, _) => ("running", None),
        (S::Cancelled, Some(R::CancelledByPerson)) => ("cancelled", None),
        (S::Cancelled, _) | (S::Interrupted, _) => ("interrupted", None),
        (S::Blocked, _) => ("tool_not_found", None),
        (S::NotRun, _) => ("not_run", None),
        (S::Completed, reason) => {
            if reason == Some(R::InputsChanged) {
                // The files moved under it: whatever it said is about a tree
                // that is no longer on disk. Never a pass.
                ("checked_nothing", Some("inputs_changed"))
            } else if thruline_core::run_execution::dependencies_unavailable(
                record.state,
                record.reason,
            ) {
                // OFFLINE-BUILD-OUTCOME (D-051): the build could not fetch
                // what it needs, so it never reached the project's code. Its
                // non-zero exit is about this machine: it checked nothing,
                // and it is never "failed". The coverage layer reads the same
                // field through the same function.
                ("checked_nothing", Some("prerequisite_absent"))
            } else if signal.is_some() {
                ("interrupted", None)
            } else {
                match exit_code {
                    None => ("checked_nothing", Some("outcome_unreadable")),
                    Some(code) if code != 0 => ("failed", None),
                    Some(_) if !streams_whole => ("checked_nothing", Some("output_not_whole")),
                    Some(_) => match tally {
                        Some(t) if t.counts_a_suite() => ("passed", None),
                        _ => ("checked_nothing", Some("no_test_ran")),
                    },
                }
            }
        }
    };
    Ok(serde_json::json!({
        "run": record.run_key,
        "state": record.state.as_str(),
        "outcome": outcome,
        "why": why,
        "reason": record.reason.map(|r| r.as_str()),
        "exit_code": exit_code,
        "signal": signal,
        "tests_passed": tally.map(|t| t.passed),
        "tests_failed": tally.map(|t| t.failed),
        "suites": tally.map(|t| t.suites),
        "failure_line": failure_line,
        "fetch_skipped": fetch_skipped,
        "started_at": record.started_at,
        "recorded_at": record.recorded_at,
        // C30 R-RUN: whole seconds from the run's own recorded start to now
        // while it runs, and to its recorded end once it has ended.
        "elapsed_s": elapsed_of(record, started.as_deref()),
        "cleanup_verified": record.cleanup_verified,
        "survivors": record.survivors,
        // B7.6, served so the screen never re-derives it.
        "settles": record.settles,
    }))
}

/* ── C30 R-RUN: the run going now, as it stands this second ─────────────── */

/// When a run began, as its own record says: the earliest start the wrapper
/// recorded for it.
///
/// A check the sheet starts is usually two phases — ThruLine's own dependency
/// fetch, then the check — and each phase's `running` row carries its own
/// `started_at`, so the execution record's latest row says when the *second*
/// phase began. Each phase also appends a `COMMAND_STARTED` observation whose
/// `observed_at` is that same `started_at` (`evidence_binding::
/// record_command_started_with_metadata`), so the earliest of those, and of
/// the latest row's own, is when the run began. Nothing here is a clock HQ
/// kept: a run with no recorded start has none.
fn recorded_start(
    ledger: &thruline_core::ledger::Ledger,
    record: &thruline_core::run_execution::ExecutionRecord,
) -> Result<Option<String>> {
    use thruline_core::hq::parse_ms;
    use thruline_core::observation::{self, ObservationType};
    let mut earliest: Option<(i64, String)> = record
        .started_at
        .as_deref()
        .and_then(|at| parse_ms(at).map(|ms| (ms, at.to_string())));
    for row in observation::for_session(ledger, &record.run_key)? {
        if row.source_adapter != "thruline"
            || row.observation_type() != Some(ObservationType::CommandStarted)
        {
            continue;
        }
        if let Some(ms) = parse_ms(&row.observed_at) {
            if earliest.as_ref().is_none_or(|(at, _)| ms < *at) {
                earliest = Some((ms, row.observed_at.clone()));
            }
        }
    }
    Ok(earliest.map(|(_, at)| at))
}

/// Whole seconds from a run's recorded start to now while the record says it
/// is going, and to the moment the record says it ended once it has. `None`
/// when there is no recorded start, or the moments are not in order.
fn elapsed_of(
    record: &thruline_core::run_execution::ExecutionRecord,
    started: Option<&str>,
) -> Option<i64> {
    use thruline_core::hq::parse_ms;
    let from = parse_ms(started?)?;
    let to = if record.state.is_open() {
        i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis(),
        )
        .ok()?
    } else {
        parse_ms(&record.recorded_at)?
    };
    (to >= from).then(|| (to - from) / 1000)
}

/// A line a check printed, as Sam reads it: the home folder as `~`, as
/// [`shown`] gives a folder. Only a whole folder name is shortened — the home
/// folder followed by `/`, or ending the path — never a longer name that
/// happens to begin with it.
fn homeless(line: &str) -> String {
    let Some(home) = home().map(|h| h.display().to_string()) else {
        return line.to_string();
    };
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return line.to_string();
    }
    let path_char = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/');
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(home) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + home.len()..].chars().next();
        let whole =
            !before.is_some_and(path_char) && after.is_none_or(|c| c == '/' || !path_char(c));
        out.push_str(&rest[..at]);
        out.push_str(if whole { "~" } else { home });
        rest = &rest[at + home.len()..];
    }
    out.push_str(rest);
    out
}

/// Reconstruct an absolute home path only inside the temporary exclusion
/// matching copy. The served run sheet keeps its existing `~/...` spelling on
/// a miss. A `~` inside a larger path component is not a home marker.
fn home_expanded_for_matching(text: &str, home: &str) -> Option<String> {
    let home = home.trim_end_matches('/');
    if home.is_empty() || !text.contains("~/") {
        return None;
    }
    let path_char = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/');
    let mut out = String::with_capacity(text.len() + home.len());
    let mut rest = text;
    let mut changed = false;
    while let Some(at) = rest.find("~/") {
        let before = rest[..at].chars().next_back();
        out.push_str(&rest[..at]);
        if before.is_some_and(path_char) {
            out.push_str("~/");
        } else {
            out.push_str(home);
            out.push('/');
            changed = true;
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

fn home_shadows(value: &serde_json::Value, home: &str, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(expanded) = home_expanded_for_matching(text, home) {
                out.push(expanded);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                home_shadows(item, home, out);
            }
        }
        serde_json::Value::Object(fields) => {
            for item in fields.values() {
                home_shadows(item, home, out);
            }
        }
        _ => {}
    }
}

/// What a run of this Change has printed so far, as the sheet serves it: the
/// newest lines ([`check::LIVE_LINES`], [`check::LIVE_BYTES`]), oldest first;
/// `cut` when anything printed is not among them as printed; every line with
/// the home folder as `~` and through `redact::mask_recorded`, with the record
/// of that masking beside them as every HQ document carries it. The lines are
/// masked in one pass, so a key printed across several lines (an armoured
/// block) is masked whole.
pub(super) fn output_json(live: &check::LiveOutput) -> serde_json::Value {
    let printed = live.read();
    let mut cut = printed.cut;
    let (lines, masking) = if printed.lines.is_empty() {
        (Vec::new(), thruline_core::redact::MaskRecord::default())
    } else {
        let text = printed
            .lines
            .iter()
            .map(|line| homeless(line))
            .collect::<Vec<_>>()
            .join("\n");
        let (text, record) = thruline_core::redact::mask_recorded(&text);
        // Masked first, cut second: the tail keeps more of a line than is served
        // (`check::LIVE_TAIL_LINE_BYTES`), so a secret is whole when it is masked
        // and the cut falls on text that has already been through the masker.
        let mut shown = Vec::new();
        for line in text.split('\n') {
            if line.len() <= check::LIVE_LINE_BYTES {
                shown.push(line.to_string());
                continue;
            }
            let mut at = check::LIVE_LINE_BYTES;
            while !line.is_char_boundary(at) {
                at -= 1;
            }
            shown.push(line[..at].to_string());
            cut = true;
        }
        (shown, record)
    };
    serde_json::json!({
        "lines": lines,
        "cut": cut,
        "masking": masking,
    })
}

/* ── GET: the run sheet ─────────────────────────────────────────────────── */

fn checks_for(state: &State, profile: &[String]) -> (Vec<&'static RunnableCheck>, Vec<String>) {
    let mut runnable = Vec::new();
    let mut not_here = Vec::new();
    for id in profile {
        match state.run_route.runnable(id) {
            Some(check) if !runnable.iter().any(|c: &&RunnableCheck| c.id == check.id) => {
                runnable.push(check)
            }
            Some(_) => {}
            None => not_here.push(masked(id)),
        }
    }
    (runnable, not_here)
}

/// Filter only the sheet's display strings. A check command can name an
/// excluded path, but replacing its whole row would also replace the run's
/// identity and outcome. Isolated entries keep that context while the shared
/// matcher still handles commands, paths, generated files, and printed lines.
fn present_run_view(
    ledger: &thruline_core::ledger::Ledger,
    project_id: &str,
    view: &mut serde_json::Value,
) -> Result<()> {
    use thruline_core::capture_exclusion::LEFT_OUT_FIELD;

    let root = crate::exclusion_serve::project_roots(ledger)?
        .remove(project_id)
        .with_context(|| format!("run Change project {project_id} has no recorded root"))?;
    let policy = crate::exclusion_serve::policy(ledger, &root)?;
    let live_check = view
        .pointer("/job/check")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);

    let mut slots: Vec<Vec<(String, &'static str)>> = vec![
        vec![("/project_folder".into(), "path")],
        vec![("/permission/granted_by".into(), "text")],
        vec![("/consent/file".into(), "file")],
        vec![("/not_started/reason".into(), "text")],
        vec![("/actor".into(), "text")],
    ];
    if let Some(rows) = view["consent"]["history"].as_array() {
        for n in 0..rows.len() {
            slots.push(vec![(format!("/consent/history/{n}/actor"), "text")]);
        }
    }
    let mut live_output_grouped = false;
    if let Some(rows) = view["checks"].as_array() {
        for (n, row) in rows.iter().enumerate() {
            let mut group = vec![
                (format!("/checks/{n}/command"), "command"),
                (format!("/checks/{n}/last/failure_line"), "failure_line"),
                (format!("/checks/{n}/last/fetch_skipped"), "fetch_skipped"),
            ];
            if live_check
                .as_deref()
                .is_some_and(|id| row["id"].as_str() == Some(id))
            {
                group.push(("/job/output/lines".into(), "lines"));
                live_output_grouped = true;
            }
            slots.push(group);
        }
    }
    if !live_output_grouped {
        slots.push(vec![("/job/output/lines".into(), "lines")]);
    }
    if let Some(rows) = view["environments"].as_array() {
        for n in 0..rows.len() {
            slots.push(vec![(format!("/environments/{n}/folder"), "path")]);
        }
    }

    // One shared pass also batches generated-file Git attribute lookups.
    // Each check's command and its display output form one entry, so a
    // matching command hides an associated line that omits the path. Run
    // identities, outcomes and counts are outside the projected entries.
    let mut positions = Vec::new();
    let mut entries = Vec::new();
    let home_for_matching = home().map(|path| path.to_string_lossy().to_string());
    for group in slots {
        let mut present = Vec::new();
        let mut entry = serde_json::Map::new();
        for (pointer, field) in group {
            if let Some(value) = view.pointer(&pointer).filter(|value| !value.is_null()) {
                entry.insert(field.to_owned(), value.clone());
                present.push((pointer, field));
            }
        }
        if !entry.is_empty() {
            if let Some(home) = home_for_matching.as_deref() {
                let mut shadows = Vec::new();
                for value in entry.values() {
                    home_shadows(value, home, &mut shadows);
                }
                if !shadows.is_empty() {
                    entry.insert("summary".into(), serde_json::json!(shadows));
                }
            }
            positions.push(present);
            entries.push(serde_json::Value::Object(entry));
        }
    }
    let mut projected = serde_json::Value::Array(entries);
    let left_out = crate::exclusion_serve::checked_apply(&mut projected, &policy)?;
    for (n, group) in positions.into_iter().enumerate() {
        for (pointer, field) in group {
            let replacement = projected[n][field].take();
            *view
                .pointer_mut(&pointer)
                .context("a run display field disappeared during exclusion")? = replacement;
        }
    }
    if left_out > 0 {
        view[LEFT_OUT_FIELD] = serde_json::Value::from(left_out);
    }
    Ok(())
}

fn view(state: &State, change_id: &str, stream: &mut TcpStream) -> Result<()> {
    let Some(ledger) = super::open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let Some(change) = thruline_core::change::read_change(&ledger, change_id)? else {
        return refuse(stream, 404, "no_such_change");
    };
    let root = project_root(&ledger, change_id)?;
    let profile = thruline_core::change_propose::accepted_check_profile(&ledger, change_id)?;
    let (runnable, not_here) = checks_for(state, &profile);
    let programs: Vec<&'static str> = runnable.iter().map(|c| c.program).collect();

    let consent_state = match &root {
        Some(r) => Some(consent(&ledger, &state.db, r)?),
        None => None,
    };
    let consent_history = match &root {
        Some(r) => thruline_core::run_permission::history(&ledger, &r.display().to_string())?,
        None => Vec::new(),
    };
    let job = match &root {
        Some(root) => state
            .run_route
            .jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("the run job table is unavailable"))?
            .get(root)
            .cloned(),
        None => None,
    };
    let remembered = match &root {
        Some(root) => state
            .run_route
            .remembered
            .lock()
            .map_err(|_| anyhow::anyhow!("the run environment table is unavailable"))?
            .get(root)
            .copied(),
        None => None,
    };
    let records = thruline_core::run_execution::all(&ledger, Some(change_id))?;

    let mut checks = Vec::new();
    for c in &runnable {
        let argv = c.argv();
        let last = match &root {
            Some(root) => {
                let name = check::run_name(&argv, root);
                match records.iter().rev().find(|r| r.name == name) {
                    Some(record) => Some(outcome_of(&ledger, record)?),
                    None => None,
                }
            }
            None => None,
        };
        checks.push(serde_json::json!({
            "id": c.id,
            "program": c.program,
            "category": thruline_core::change_propose::profile_check_category(c.id),
            // Technical face only: the argv exactly as the wrapper runs it.
            "command": masked(&thruline_core::evidence_binding::rendered_command(&argv)),
            "last": last,
        }));
    }

    let job_json = match &job {
        Some(job) => {
            let record = records.iter().find(|r| r.run_key == job.run_key);
            let this_change = job.change_id == change_id;
            // C30 R-RUN: this Change's run only — another Change's output is
            // never served as this one's — and the elapsed time from the
            // run's own recorded start, so nothing before the wrapper's first
            // row has one.
            let started = match (this_change, record) {
                (true, Some(record)) => recorded_start(&ledger, record)?,
                _ => None,
            };
            serde_json::json!({
                "run": job.run_key,
                "check": job.check,
                "for_this_change": this_change,
                "phase": if record.is_some() { "running" } else { "starting" },
                "environment": job.environment,
                "started_at": job.started_at,
                "recorded_started_at": started,
                "elapsed_s": record.and_then(|r| elapsed_of(r, started.as_deref())),
                "output": if this_change {
                    output_json(&job.live)
                } else {
                    serde_json::Value::Null
                },
            })
        }
        None => serde_json::Value::Null,
    };
    let not_started = state
        .run_route
        .not_started
        .lock()
        .map_err(|_| anyhow::anyhow!("the run table is unavailable"))?
        .get(change_id)
        .cloned()
        .map(|n| {
            serde_json::json!({
                "run": n.run_key,
                "check": n.check,
                // Technical face only.
                "reason": masked(&n.reason),
            })
        });

    let mut served = serde_json::json!({
        "schema_version": RUN_VIEW_SCHEMA_VERSION,
        "change": change_id,
        "project_folder": root.as_deref().map(shown),
        // Non-null only when the record and its cache agree (P6-5B).
        "permission": match &consent_state {
            Some(ConsentState::Given { grant }) => serde_json::json!({
                "granted_by": masked(&grant.actor),
                "granted_at": grant.recorded_at,
            }),
            _ => serde_json::Value::Null,
        },
        // P6-5B — the consent decision and its history, from the record.
        "consent": {
            "state": consent_state.as_ref().map(ConsentState::word),
            "history": consent_history.iter().map(consent_row_json).collect::<Vec<_>>(),
            // Technical face only: where the cache lives.
            "file": PERMISSION_FILE,
        },
        "checks": checks,
        "not_runnable_here": not_here,
        "environments": match &root {
            Some(root) => environments_found(&state.run_route, root, &programs),
            None => Vec::new(),
        },
        "remembered_environment": remembered,
        "job": job_json,
        "not_started": not_started,
        // Who a Cancel is recorded as: the account HQ runs as.
        "actor": masked(&check::account_name()),
    });
    present_run_view(&ledger, &change.project_id, &mut served)?;
    json(stream, 200, &served)
}

/* ── POST: permission ───────────────────────────────────────────────────── */

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantRequest {
    /// Settings supplies the opaque identity it read. The Check sheet's `{}`
    /// remains valid and uses the Change's current recorded folder as before.
    #[serde(default)]
    expected_folder: Option<String>,
    #[serde(default)]
    expected_row: Option<String>,
}

impl GrantRequest {
    fn parse(body: &[u8]) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_slice(body).ok()?;
        let fields = value.as_object()?;
        if !fields.is_empty()
            && (fields.len() != 2
                || !fields.get("expected_folder")?.is_string()
                || !fields.contains_key("expected_row"))
        {
            return None;
        }
        let request: Self = serde_json::from_value(value).ok()?;
        request.valid().then_some(request)
    }

    fn valid(&self) -> bool {
        (self.expected_folder.is_some() || self.expected_row.is_none())
            && self
                .expected_folder
                .as_ref()
                .is_none_or(|id| id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
            && self
                .expected_row
                .as_ref()
                .is_none_or(|id| !id.trim().is_empty())
    }

    fn matches(&self, folder: &str, latest: Option<&str>) -> bool {
        self.expected_folder.as_ref().is_none_or(|expected| {
            expected == &folder_id(folder) && self.expected_row.as_deref() == latest
        })
    }

    fn guarded(&self) -> bool {
        self.expected_folder.is_some()
    }
}

pub(super) fn target_changed(stream: &mut TcpStream) -> Result<()> {
    json(
        stream,
        409,
        &serde_json::json!({
            "error": "permission_target_changed",
            "plain": "This permission changed in the record. Read it again."
        }),
    )
}

fn grant(state: &State, change_id: &str, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    let Some(request) = GrantRequest::parse(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    let Some(ledger) = super::open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if thruline_core::change::read_change(&ledger, change_id)?.is_none() {
        return refuse(stream, 404, "no_such_change");
    }
    // ADV-run-ledger-4: from here to the file, one consent act at a time.
    let _one_act = state
        .run_route
        .consent_acts
        .lock()
        .map_err(|_| anyhow::anyhow!("the permission table is unavailable"))?;
    let Some(root) = project_root(&ledger, change_id)? else {
        return if request.guarded() {
            target_changed(stream)
        } else {
            refuse(stream, 409, "no_project_folder")
        };
    };
    // The recorded root itself must still be a plain directory. This check
    // refuses even an idempotent press over a replaced symlink; `prepare`
    // opens the root with NOFOLLOW again before the file write.
    match place::pin_for_revoke(&root, false) {
        Ok(_) => {}
        Err(Place::NotPlain) => return refuse(stream, 409, "permission_file_not_plain"),
        Err(Place::Io(e)) => return Err(e).context("opening the project's recorded folder"),
    }
    let (standing, seen) = consent_and_latest(&ledger, &state.db, &root)?;
    let folder = root.display().to_string();
    if !request.matches(&folder, seen.as_deref()) {
        return target_changed(stream);
    }
    if let ConsentState::Given { grant } = standing {
        return json(
            stream,
            200,
            &serde_json::json!({
                "granted": true,
                "granted_by": masked(&grant.actor),
                "granted_at": grant.recorded_at,
            }),
        );
    }
    // ADV-run-security-1 — inspect the cache destination before the row. A
    // Settings request does this without creating a directory or seal key:
    // a stale cross-process row refusal must leave no new file behind. The
    // legacy Check-sheet act keeps its existing pre-append preparation.
    let (ready_dir, pinned_grant, ready_key) = if request.guarded() {
        let pinned = match place::inspect_for_grant(&root) {
            Ok(pinned) => pinned,
            Err(Place::NotPlain) => return refuse(stream, 409, "permission_file_not_plain"),
            Err(Place::Io(e)) => {
                return Err(e).context("inspecting the project's .thruline folder");
            }
        };
        (None, Some(pinned), seal_key(&state.db, false)?)
    } else {
        let dir = match place::prepare(&root) {
            Ok(dir) => dir,
            Err(Place::NotPlain) => return refuse(stream, 409, "permission_file_not_plain"),
            Err(Place::Io(e)) => {
                return Err(e).context("opening the project's .thruline folder");
            }
        };
        (
            Some(dir),
            None,
            Some(seal_key(&state.db, true)?.context("the seal key was not created")?),
        )
    };
    let by = check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    // P6-5B — the record first. If the file cannot be written after this, the
    // record holds a grant with no cache, which refuses (`FileMissing`): the
    // failure can only withhold a run, never start one.
    let project_id = thruline_core::change::read_change(&ledger, change_id)?
        .map(|c| c.project_id)
        .context("the Change went away while permission was being given")?;
    // ADV-run-ledger-4: appended only over the row this answer rested on,
    // re-read inside the append's own transaction. If another writer on this
    // store acted first, nothing is appended: a grant that now stands is the
    // answer, and one that was taken back is refused, as a run would be.
    use thruline_core::run_permission::{Granting, Standing};
    let row = match thruline_core::run_permission::grant_over(
        &ledger,
        &thruline_core::run_permission::Consent {
            project_id: &project_id,
            project_folder: &folder,
            actor: &by,
            recorded_at: &at,
        },
        seen.as_deref(),
    )? {
        Granting::Appended(row) => row,
        Granting::Moved(Standing::Granted(row)) => {
            if request.guarded() {
                return target_changed(stream);
            }
            return json(
                stream,
                200,
                &serde_json::json!({
                    "granted": true,
                    "granted_by": masked(&row.actor),
                    "granted_at": row.recorded_at,
                    "grant": consent_row_json(&row),
                }),
            );
        }
        Granting::Moved(Standing::Revoked(_) | Standing::Never) => {
            if request.guarded() {
                return target_changed(stream);
            }
            return refuse(stream, 409, "permission_revoked");
        }
    };
    // For Settings, prepare only after the row comparison and append. Any
    // path change now leaves a grant without a usable cache, so it cannot run;
    // the response is uncertain and the client re-reads the row.
    let dir = match ready_dir {
        Some(dir) => dir,
        None => match place::prepare_pinned(pinned_grant.expect("guarded grant has pinned root")) {
            Ok(dir) => dir,
            Err(Place::NotPlain) => {
                anyhow::bail!("the permission cache path changed after the grant row")
            }
            Err(Place::Io(e)) => {
                return Err(e)
                    .context("opening the project's .thruline folder after the grant row");
            }
        },
    };
    let key = match ready_key {
        Some(key) => key,
        None => seal_key(&state.db, true)?.context("the seal key was not created")?,
    };
    let file = PermissionFile {
        schema_version: PERMISSION_FILE_VERSION,
        seal: seal(&key, &folder, &by, &at, &row.id),
        project_folder: folder,
        granted_by: by,
        granted_at: at,
        grant_id: row.id.clone(),
        promise: PROMISE.to_string(),
    };
    keep_out_of_the_version(&dir)?;
    place::write(&dir, &serde_json::to_vec_pretty(&file)?).context("writing the run permission")?;
    json(
        stream,
        200,
        &serde_json::json!({
            "granted": true,
            "granted_by": masked(&file.granted_by),
            "granted_at": file.granted_at,
            "grant": consent_row_json(&row),
        }),
    )
}

/* ── POST: take the permission back (P6-5B) ────────────────────────────── */

/// Take back the standing grant for this project's folder: one row appended,
/// pointing at the grant, THEN the cache removed. With no standing grant in
/// the record there is nothing to point at, so nothing is appended and the
/// answer says so. A cache that is already gone is not an error — the record
/// is what was asked to change.
fn revoke(state: &State, change_id: &str, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    let Some(request) = GrantRequest::parse(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    let Some(ledger) = super::open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let Some(change) = thruline_core::change::read_change(&ledger, change_id)? else {
        return refuse(stream, 404, "no_such_change");
    };
    // ADV-run-ledger-4: the row and the removal of its copy, as one act.
    let _one_act = state
        .run_route
        .consent_acts
        .lock()
        .map_err(|_| anyhow::anyhow!("the permission table is unavailable"))?;
    let Some(root) = project_root(&ledger, change_id)? else {
        return if request.guarded() {
            target_changed(stream)
        } else {
            refuse(stream, 409, "no_project_folder")
        };
    };
    let pinned = match if request.guarded() {
        place::pin_for_revoke(&root, false)
    } else {
        place::pin_root_for_legacy_revoke(&root)
    } {
        Ok(pinned) => pinned,
        Err(Place::NotPlain) => return refuse(stream, 409, "permission_file_not_plain"),
        Err(Place::Io(e)) => return Err(e).context("opening the project's recorded folder"),
    };
    let folder = root.display().to_string();
    let standing = thruline_core::run_permission::standing(&ledger, &folder)?;
    if !request.matches(&folder, thruline_core::run_permission::latest_id(&standing)) {
        return target_changed(stream);
    }
    let by = check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    let consent = thruline_core::run_permission::Consent {
        project_id: &change.project_id,
        project_folder: &folder,
        actor: &by,
        recorded_at: &at,
    };
    let row = if request.guarded() {
        match thruline_core::run_permission::revoke_over(
            &ledger,
            &consent,
            request.expected_row.as_deref(),
        )? {
            thruline_core::run_permission::Revoking::Appended(row) => row,
            thruline_core::run_permission::Revoking::Moved(_) => return target_changed(stream),
        }
    } else {
        let Some(row) = thruline_core::run_permission::revoke(&ledger, &consent)? else {
            return refuse(stream, 409, "no_permission_to_revoke");
        };
        row
    };
    // ADV-run-security-1 — removed from a `.thruline` opened without following
    // a link, so a linked folder never carries the removal outside the project.
    remove_after_revoke(&pinned);
    json(
        stream,
        200,
        &serde_json::json!({
            "revoked": true,
            "revoked_by": masked(&row.actor),
            "revoked_at": row.recorded_at,
            "revocation": consent_row_json(&row),
        }),
    )
}

fn remove_after_revoke(pinned: &place::PinnedRemoval) {
    match place::remove_pinned(pinned) {
        Ok(()) => {}
        // The record already says it was taken back, and the record wins: a
        // file left behind is refused on every run. Said, never hidden.
        Err(Place::NotPlain) => tracing::warn!(
            "the run permission file was not removed: .thruline is a link or not a folder, and is never entered"
        ),
        Err(Place::Io(e)) => tracing::warn!("the run permission file was not removed: {e}"),
    }
}

/// Settings' Take back for a standing historical grant whose exact recorded
/// folder no current usable Change names. The browser supplies only an opaque
/// identity and the latest row it saw; both are resolved again under the act
/// lock, and the row comparison is repeated inside the append transaction.
pub(super) fn revoke_historical(
    state: &State,
    ledger: &thruline_core::ledger::Ledger,
    project: &thruline_core::hq::HqProject,
    id: &str,
    expected_row: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    let _one_act = state
        .run_route
        .consent_acts
        .lock()
        .map_err(|_| anyhow::anyhow!("the permission table is unavailable"))?;
    // HQ membership was first read before this lock to select the route.
    // Re-derive it here so a newly grouped or moved ledger project cannot
    // make an old browser folder identity authorize another project's row.
    let Some(project) = thruline_core::hq::find_project(ledger, &project.id)? else {
        return target_changed(stream);
    };
    let (folder, project_id) =
        match super::run_permission_settings::historical_target(ledger, &project, id)? {
            super::run_permission_settings::HistoricalTarget::NotFound => {
                return refuse(stream, 404, "no_such_permission_folder")
            }
            super::run_permission_settings::HistoricalTarget::Moved => {
                return target_changed(stream)
            }
            super::run_permission_settings::HistoricalTarget::Found { folder, project_id } => {
                (folder, project_id)
            }
        };
    let root = Path::new(&folder);
    if !root.is_absolute()
        || root
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        return target_changed(stream);
    }
    let pinned = match place::pin_for_revoke(root, true) {
        Ok(pinned) => pinned,
        Err(Place::NotPlain) => return target_changed(stream),
        Err(Place::Io(e)) => return Err(e).context("opening the recorded permission folder"),
    };
    let grant = match thruline_core::run_permission::standing(ledger, &folder)? {
        thruline_core::run_permission::Standing::Granted(grant)
            if grant.id == expected_row && grant.project_id == project_id =>
        {
            grant
        }
        _ => return target_changed(stream),
    };
    let by = check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    let row = match thruline_core::run_permission::revoke_over(
        ledger,
        &thruline_core::run_permission::Consent {
            project_id: &grant.project_id,
            project_folder: &folder,
            actor: &by,
            recorded_at: &at,
        },
        Some(expected_row),
    )? {
        thruline_core::run_permission::Revoking::Appended(row) => row,
        thruline_core::run_permission::Revoking::Moved(_) => return target_changed(stream),
    };
    remove_after_revoke(&pinned);
    json(
        stream,
        200,
        &serde_json::json!({
            "revoked": true,
            "revoked_by": masked(&row.actor),
            "revoked_at": row.recorded_at,
            "revocation": consent_row_json(&row),
        }),
    )
}

/// What `.thruline/.gitignore` says when ThruLine writes it.
pub(super) const IGNORE_FILE_TEXT: &str =
    "# ThruLine: this machine's permission to run checks. It is sealed to this\n\
# machine's store, so it is never part of the project and never committed.\n\
/run-permission.json\n\
/.gitignore\n";

/// The permission is this machine's, not the project's: writing it must not
/// move the project's version. The version counts untracked files, so a new
/// file under the project would make every earlier run read as "ran on an
/// earlier version" the moment Sam said yes. A `.gitignore` beside it that
/// names only itself and the permission keeps both out of the version. One
/// that already exists is the project's own and is left exactly as it is —
/// and so is a link at that name, which is never followed (ADV-run-security-1).
fn keep_out_of_the_version(dir: &place::Dir) -> Result<()> {
    place::create_new(dir, ".gitignore", IGNORE_FILE_TEXT.as_bytes())
        .map(|_| ())
        .context("writing .thruline/.gitignore")
}

/* ── POST: start one run ────────────────────────────────────────────────── */

/// **Guard 3, by shape.** A check id and, optionally, an environment id. A
/// request with a `command`, `args`, `path`, `cwd`, `env` — or any other field
/// — does not parse, and is refused whole.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartRequest {
    check: String,
    #[serde(default)]
    environment: Option<String>,
}

fn start(state: &Arc<State>, change_id: &str, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    let Ok(request) = serde_json::from_slice::<StartRequest>(body) else {
        return refuse(stream, 400, "check_id_only");
    };
    if state.shutting_down.load(Ordering::SeqCst) {
        return refuse(stream, 503, "hq_is_stopping");
    }
    let Some(ledger) = super::open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if thruline_core::change::read_change(&ledger, change_id)?.is_none() {
        return refuse(stream, 404, "no_such_change");
    }
    let Some(root) = project_root(&ledger, change_id)? else {
        return refuse(stream, 409, "no_project_folder");
    };
    // Guard 5, and P6-5B: the record and its cache must agree.
    if let Some(refusal) = consent(&ledger, &state.db, &root)?.refusal() {
        return refuse(stream, 403, refusal);
    }
    // Guard 4: in the accepted profile, AND allow-listed with a fixed argv.
    let profile = thruline_core::change_propose::accepted_check_profile(&ledger, change_id)?;
    if !profile.iter().any(|id| id == &request.check) {
        return refuse(stream, 403, "not_in_check_profile");
    }
    let Some(runnable) = state.run_route.runnable(&request.check) else {
        return refuse(stream, 403, "not_runnable_from_here");
    };
    let (environment_id, place) = match request.environment.as_deref() {
        None => ("hq", Where::Hq),
        Some(id) => match environment(id) {
            Some(found) => found,
            None => return refuse(stream, 400, "unknown_environment"),
        },
    };
    if let Some(dir) = state.run_route.environment_dir(&root, place) {
        if !dir.is_dir() {
            return refuse(stream, 409, "environment_absent");
        }
    }
    let argv = runnable.argv();
    // Guard 4 (ADV-run-security-2): the program is looked for the way the
    // child would look, and a program the repository put there is refused
    // before anything starts, is recorded nowhere and is not remembered —
    // it never runs, so nothing can read its word as a result. The same holds
    // for the program rustup's proxy would hand the run to
    // (ADV-run-security-3).
    let path = match resolve(
        &state.run_route,
        runnable.program,
        &root,
        &state.run_route.search_path(&root, place),
    ) {
        Found::InProject(at) => {
            return presented_path_refusal(
                stream,
                &ledger,
                change_id,
                &at,
                &serde_json::json!({
                    "error": "program_in_project",
                    "program": runnable.program,
                    // Technical face only: where it was found, or where
                    // rustup's proxy would have handed the run.
                    "found_at": shown(&at),
                }),
            );
        }
        Found::Undecided { why, at } => {
            return presented_path_refusal(
                stream,
                &ledger,
                change_id,
                &at,
                &serde_json::json!({
                    "error": "not_runnable_from_here",
                    "program": runnable.program,
                    // Technical face only: why the guard could not tell which
                    // program would start, and what it was looking at.
                    "why": why,
                    "found_at": shown(&at),
                }),
            );
        }
        Found::Outside(path, toolchain) => Some((path, toolchain)),
        Found::Nowhere => None,
    };

    // The double-click guard: one lock across "is one going?" and "this one is".
    let mut jobs = state
        .run_route
        .jobs
        .lock()
        .map_err(|_| anyhow::anyhow!("the run job table is unavailable"))?;
    if let Some(going) = jobs.get(&root) {
        return json(
            stream,
            409,
            &serde_json::json!({
                "error": "already_running",
                "run": going.run_key,
                "check": going.check,
                "for_this_change": going.change_id == change_id,
            }),
        );
    }
    let started_at = thruline_core::ingest::iso8601_utc_now();
    let run_key = mint_run_key();
    if let Ok(mut remembered) = state.run_route.remembered.lock() {
        remembered.insert(root.clone(), environment_id);
    }

    // Tool not found: recorded as a check that could not start, and nothing
    // is installed. The program is looked for the way the child would look.
    let Some((path, toolchain)) = path else {
        drop(jobs);
        record_blocked(
            &ledger,
            change_id,
            &run_key,
            &check::run_name(&argv, &root),
            &started_at,
        )?;
        return json(
            stream,
            200,
            &serde_json::json!({
                "run": run_key,
                "outcome": "tool_not_found",
                "program": runnable.program,
            }),
        );
    };

    let live = Arc::new(check::LiveOutput::new());
    jobs.insert(
        root.clone(),
        Job {
            change_id: change_id.to_string(),
            check: request.check.clone(),
            run_key: run_key.clone(),
            environment: environment_id,
            started_at,
            live: Arc::clone(&live),
        },
    );
    drop(jobs);

    let worker_state = Arc::clone(state);
    let worker_change = change_id.to_string();
    let worker_key = run_key.clone();
    let worker_check = request.check.clone();
    let spawned = std::thread::Builder::new()
        .name("thruline-run".into())
        .spawn(move || {
            run_one(
                &worker_state,
                &worker_change,
                &worker_check,
                &worker_key,
                &root,
                argv,
                (path, toolchain),
                live,
            )
        });
    if let Err(e) = spawned {
        // Nothing started; release the slot so the next press can try again.
        if let Ok(mut jobs) = state.run_route.jobs.lock() {
            jobs.retain(|_, j| j.run_key != run_key);
        }
        return Err(anyhow::anyhow!("the run could not be started: {e}"));
    }
    json(
        stream,
        202,
        &serde_json::json!({ "run": run_key, "check": request.check }),
    )
}

/// A run key the screen can put in an address exactly as it is.
///
/// The screen names a run in the T-3-4 stop address through
/// `encodeURIComponent`, and that route matches the segment as sent, without
/// decoding it. A key holding `:` (the terminal door's `cli:check:<time>`)
/// arrives as `%3A` and names no run, so Cancel would silently not cancel.
/// This key is only letters, digits and `-`, which `encodeURIComponent`
/// leaves alone; the ULID carries the time, and `started_at` is its own field.
pub(super) fn mint_run_key() -> String {
    format!("hq-check-{}", ulid::Ulid::generate())
}

/// A check that could not start because its program is not on this machine
/// where it was looked for (`blocked`, `prerequisite absent`). No process was
/// started, so none can remain: the cleanup answer is known, not assumed.
fn record_blocked(
    ledger: &thruline_core::ledger::Ledger,
    change_id: &str,
    run_key: &str,
    name: &str,
    at: &str,
) -> Result<()> {
    use thruline_core::run_execution as rx;
    rx::start(
        ledger,
        &rx::Start {
            run_key: run_key.to_string(),
            change_id: Some(change_id.to_string()),
            name: name.to_string(),
            pid: None,
            process_group: None,
            started_at: at.to_string(),
            examined_inputs_id: None,
        },
    )?;
    rx::finish(
        ledger,
        &rx::Finish {
            run_key: run_key.to_string(),
            state: rx::ExecutionState::Blocked,
            reason: Some(rx::Reason::PrerequisiteAbsent),
            actor: None,
            binding_id: None,
            cleanup_verified: Some(true),
            survivors: Some(0),
            at: at.to_string(),
        },
    )?;
    Ok(())
}

/// The worker: the `thruline check` wrapper, in this process, for this run.
#[allow(clippy::too_many_arguments)]
fn run_one(
    state: &State,
    change_id: &str,
    check_id: &str,
    run_key: &str,
    root: &Path,
    argv: Vec<String>,
    // The search path guard 4 composed, and the toolchain it checked
    // (ADV-run-security-7), which the run is pinned to.
    (path, toolchain): (std::ffi::OsString, Option<std::ffi::OsString>),
    // C30 R-RUN: where the wrapper passes the run's streams through, which
    // the sheet reads while the job lasts.
    live: Arc<check::LiveOutput>,
) {
    let refusal = Arc::new(Mutex::new(None));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check::run_check_at(
            &state.db,
            check::CheckArgs {
                shell: false,
                quiet: true,
                artifacts: Vec::new(),
                app_port: None,
                config_files: Vec::new(),
                for_change: Some(change_id.to_string()),
                actor: None,
                evidence_class: None,
                command: argv,
            },
            check::Launch {
                cwd: Some(root.to_path_buf()),
                path_env: Some(path),
                rustup_toolchain: toolchain,
                run_key: Some(run_key.to_string()),
                detached: true,
                refusal: Some(Arc::clone(&refusal)),
                // ADV-run-ledger-3: the sheet promises no network; say so to
                // the program, not only to the person.
                offline: true,
                live: Some(live),
                #[cfg(test)]
                env: state.run_route.env.clone(),
            },
        )
    }));
    // A run the wrapper never recorded is SAID, never left looking like it is
    // still starting: the sheet shows that it did not start.
    let recorded = thruline_core::ledger::Ledger::open(&state.db)
        .ok()
        .and_then(|l| {
            thruline_core::run_execution::latest(&l, run_key)
                .ok()
                .flatten()
        })
        .is_some();
    if !recorded {
        let reason = match (&result, refusal.lock().ok().and_then(|r| r.clone())) {
            (_, Some(reason)) => reason,
            (Ok(Err(e)), None) => format!("{e:#}"),
            (Err(_), None) => "the run stopped unexpectedly before anything was recorded".into(),
            (Ok(Ok(_)), None) => "nothing was recorded for this run".into(),
        };
        if let Ok(mut table) = state.run_route.not_started.lock() {
            table.insert(
                change_id.to_string(),
                NotStarted {
                    run_key: run_key.to_string(),
                    check: check_id.to_string(),
                    reason,
                },
            );
        }
    } else if let Ok(mut table) = state.run_route.not_started.lock() {
        table.remove(change_id);
    }
    if let Ok(mut jobs) = state.run_route.jobs.lock() {
        jobs.retain(|_, j| j.run_key != run_key);
    }
}

#[cfg(test)]
mod exclusion_run_sheet {
    use super::*;
    use thruline_core::capture_hold::{self, Act, Hold};

    const PROJECT_ID: &str = "cccccccc-0000-4000-8000-00000000p099";

    fn ledger_with_project(root: &str, db: &Path) -> thruline_core::ledger::Ledger {
        let ledger = thruline_core::ledger::Ledger::open(db).expect("synthetic ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id, root_path, source, display_name, created_at)
                 VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
                rusqlite::params![PROJECT_ID, root],
            )
            .expect("synthetic project");
        ledger
    }

    #[test]
    fn a_home_shortened_refusal_matches_the_original_absolute_path() {
        let home = home().expect("synthetic HOME prefix");
        let root = home.join("synthetic-c46-run");
        let at = root.join("private/tool");
        let policy = thruline_core::capture_exclusion::ExclusionPolicy::from_exclusions(
            &root.to_string_lossy(),
            &[thruline_core::capture_hold::ExclusionView {
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
        .expect("synthetic policy");
        let value = serde_json::json!({
            "error": "not_runnable_from_here",
            "found_at": shown(&at),
            "why": "toolchain could not be resolved"
        });
        assert!(value["found_at"].as_str().unwrap().starts_with("~/"));
        let served = present_path_refusal_value(&value, &at, &policy).expect("presented refusal");
        assert_eq!(served["error"], "not_runnable_from_here");
        assert_eq!(
            served["found_at"],
            thruline_core::capture_exclusion::marker("private")
        );
        assert_eq!(served["why"], "toolchain could not be resolved");
        assert!(!served.to_string().contains(at.to_string_lossy().as_ref()));
    }

    #[test]
    fn a_home_shortened_live_line_and_environment_folder_are_hidden() {
        let temp = tempfile::tempdir().expect("scratch ledger folder");
        let home = home().expect("synthetic HOME prefix");
        let root = home.join("synthetic-c46-run-sheet");
        let root = root.to_string_lossy().to_string();
        let ledger = ledger_with_project(&root, &temp.path().join("ledger.db"));
        let hold = Hold {
            project_id: PROJECT_ID,
            project_folder: &root,
            actor: "synthetic person",
            recorded_at: "2026-10-08T00:00:00Z",
        };
        capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some("private"))
            .expect("recorded exclusion");
        let mut view = serde_json::json!({
            "project_folder": shown(Path::new(&root)),
            "checks": [],
            "job": {"output": {"lines": ["~/synthetic-c46-run-sheet/private/log: PRIVATE LINE", "~/synthetic-c46-run-sheet/public/log: PUBLIC LINE"]}},
            "environments": [{"folder":"~/synthetic-c46-run-sheet/private/tools"}],
        });
        present_run_view(&ledger, PROJECT_ID, &mut view).expect("presented sheet");
        assert_eq!(
            view["job"]["output"]["lines"][0],
            thruline_core::capture_exclusion::marker("private")
        );
        assert_eq!(
            view["job"]["output"]["lines"][1],
            thruline_core::capture_exclusion::marker("private")
        );
        assert_eq!(
            view["environments"][0]["folder"],
            thruline_core::capture_exclusion::marker("private")
        );
        assert_eq!(view["project_folder"], "~/synthetic-c46-run-sheet");
    }

    #[test]
    fn a_run_sheet_hides_excluded_display_lines_and_keeps_run_facts() {
        let temp = tempfile::tempdir().expect("scratch directory");
        let root = temp.path().join("project").to_string_lossy().to_string();
        let ledger = ledger_with_project(&root, &temp.path().join("ledger.db"));
        let hold = Hold {
            project_id: PROJECT_ID,
            project_folder: &root,
            actor: "sam",
            recorded_at: "2026-01-02T09:30:00Z",
        };
        capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some("private/log"))
            .expect("record path exclusion");
        let original = serde_json::json!({
            "schema_version": 3,
            "change": "change-1",
            "project_folder": root.clone(),
            "checks": [{
                "id": "cargo-test",
                "program": "sh",
                "command": "cat private/log",
                "last": {
                    "run": "run-1",
                    "state": "running",
                    "outcome": "running",
                    "tests_passed": 5,
                    "tests_failed": 0,
                    "failure_line": "canary failure without a path",
                    "fetch_skipped": "canary fetch detail without a path",
                },
            }],
            "job": {
                "run": "run-1",
                "check": "cargo-test",
                "phase": "running",
                "elapsed_s": 7,
                "output": {"lines": ["private/log: canary output", "PRIVATE_PATHLESS_BYTES"], "cut": false},
            },
            "not_started": null,
            "environments": [],
            "consent": {"history": [], "file": ".thruline/run-permission.json"},
            "permission": null,
            "actor": "sam",
        });
        let mut served = original.clone();
        present_run_view(&ledger, PROJECT_ID, &mut served).expect("present run sheet");
        let mark = thruline_core::capture_exclusion::marker("private/log");
        assert_eq!(served["checks"][0]["command"], mark);
        assert_eq!(served["checks"][0]["last"]["failure_line"], mark);
        assert_eq!(served["checks"][0]["last"]["fetch_skipped"], mark);
        assert_eq!(served["job"]["output"]["lines"][0], mark);
        assert_eq!(served["job"]["output"]["lines"][1], mark);
        assert_eq!(served["left_out_by_exclusion"], 1);
        for path in [
            "/schema_version",
            "/change",
            "/checks/0/id",
            "/checks/0/program",
            "/checks/0/last/run",
            "/checks/0/last/state",
            "/checks/0/last/outcome",
            "/checks/0/last/tests_passed",
            "/checks/0/last/tests_failed",
            "/job/run",
            "/job/check",
            "/job/phase",
            "/job/elapsed_s",
            "/job/output/cut",
        ] {
            assert_eq!(served.pointer(path), original.pointer(path), "{path}");
        }
        assert!(!served.to_string().contains("canary"), "{served}");
        assert!(
            !served.to_string().contains("PRIVATE_PATHLESS_BYTES"),
            "{served}"
        );

        let mut other_job = original.clone();
        other_job["job"]["check"] = serde_json::json!("unrelated-check");
        other_job["job"]["output"]["lines"] = serde_json::json!(["unrelated check output"]);
        present_run_view(&ledger, PROJECT_ID, &mut other_job).expect("unrelated run output");
        assert_eq!(other_job["checks"][0]["command"], mark);
        assert_eq!(
            other_job["job"]["output"]["lines"][0], "unrelated check output",
            "the current job belongs to a different check"
        );
        assert_eq!(other_job["left_out_by_exclusion"], 1);

        capture_hold::record_for_person(
            &ledger,
            Act::Included,
            &Hold {
                recorded_at: "2026-01-02T09:31:00Z",
                ..hold
            },
            Some("private/log"),
        )
        .expect("include path again");
        let mut included = original.clone();
        present_run_view(&ledger, PROJECT_ID, &mut included).expect("present included path");
        assert_eq!(
            included, original,
            "including restored the original display"
        );
    }

    #[test]
    fn a_generated_file_lookup_failure_refuses_the_run_sheet() {
        let temp = tempfile::tempdir().expect("scratch directory");
        // This folder deliberately is not a Git repository. A generated-file
        // rule cannot be checked here, so no unfiltered sheet may be served.
        let root = temp.path().join("not-a-repository");
        std::fs::create_dir_all(&root).expect("project folder");
        let root = root.to_string_lossy().to_string();
        let ledger = ledger_with_project(&root, &temp.path().join("ledger.db"));
        capture_hold::record_for_person(
            &ledger,
            Act::Excluded,
            &Hold {
                project_id: PROJECT_ID,
                project_folder: &root,
                actor: "sam",
                recorded_at: "2026-01-02T09:30:00Z",
            },
            Some("generated"),
        )
        .expect("record generated exclusion");
        let mut sheet = serde_json::json!({
            "job": {"output": {"lines": ["src/generated.rs: canary output"]}},
        });
        let before = sheet.clone();
        assert!(present_run_view(&ledger, PROJECT_ID, &mut sheet).is_err());
        assert_eq!(
            sheet, before,
            "a failed policy check cannot partially present a sheet"
        );
    }
}

/// RUN-STOP-PGID-GUARD: HQ's stop signals each question's group through the
/// guarded door. Exercised through a recording fake only, so no real signal
/// is ever delivered by this test or by a mutant of the guard.
#[cfg(all(test, unix))]
mod group_signal_guard {
    use super::RunRoute;

    #[test]
    fn stopping_hq_refuses_a_question_group_that_is_not_one_owned_group() {
        let own = rustix::process::getpgrp().as_raw_nonzero().get();
        let route = RunRoute::new();
        {
            let mut questions = route.questions.lock().expect("the question table");
            questions.groups.extend([0, 1, -1, i32::MIN, own]);
        }
        let mut calls = Vec::new();
        route.stop_questions_with(|group| {
            calls.push(group);
            Ok(())
        });
        assert!(
            calls.is_empty(),
            "a forbidden group reached the signaller: {calls:?}"
        );
        assert!(route.questions.lock().expect("the question table").stopping);
    }

    #[test]
    fn stopping_hq_signals_a_question_group_it_owns_exactly_once() {
        use std::os::unix::process::CommandExt;

        let mut child = std::process::Command::new("/bin/cat")
            .process_group(0)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("cat spawns");
        let group = i32::try_from(child.id()).expect("a pid fits i32");
        let route = RunRoute::new();
        route
            .questions
            .lock()
            .expect("the question table")
            .groups
            .insert(group);
        let mut calls = Vec::new();
        route.stop_questions_with(|g| {
            calls.push(g);
            Ok(())
        });
        drop(child.stdin.take());
        let _ = child.wait();
        assert_eq!(calls, vec![group]);
    }
}

#[cfg(all(test, unix))]
mod pinned_grant_test {
    use super::{place, PERMISSION_FILE};

    #[test]
    fn a_plain_directory_swap_after_preflight_cannot_receive_the_grant_cache() {
        let tmp = tempfile::tempdir().expect("scratch");
        let base = tmp.path().canonicalize().expect("canonical scratch");
        for had_cache_dir in [false, true] {
            let name = if had_cache_dir { "existing" } else { "absent" };
            let root = base.join(name);
            let stored = base.join(format!("{name}-stored"));
            std::fs::create_dir(&root).expect("recorded root");
            if had_cache_dir {
                std::fs::create_dir(root.join(".thruline")).expect("existing cache directory");
            }
            let pinned = place::inspect_for_grant(&root).expect("read-only preflight");
            if !had_cache_dir {
                assert!(
                    !root.join(".thruline").exists(),
                    "preflight created a cache directory"
                );
            }
            std::fs::rename(&root, &stored).expect("move recorded root");
            std::fs::create_dir_all(root.join(".thruline")).expect("replacement root");
            let sentinel = root.join(PERMISSION_FILE);
            std::fs::write(&sentinel, b"replacement sentinel").expect("replacement cache");

            let dir = place::prepare_pinned(pinned).expect("prepare pinned root");
            place::write(&dir, b"cache in pinned root").expect("write pinned cache");
            assert_eq!(
                std::fs::read(&sentinel).expect("replacement cache remains"),
                b"replacement sentinel"
            );
            assert_eq!(
                std::fs::read(stored.join(PERMISSION_FILE)).expect("old pinned cache"),
                b"cache in pinned root"
            );
        }
    }
}
