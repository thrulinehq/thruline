//! `thruline ui` — the foreground local server behind the one command.
//!
//! Deliberately not a daemon and deliberately not a framework. It is a blocking
//! `TcpListener` on the IPv4 loopback that lives exactly as long as the user's
//! command: Ctrl+C kills the process, the socket closes with it, and nothing is
//! installed or scheduled. An explicit --observe-codex source enables a bounded
//! foreground log observer owned by this process; it stops with the server.
//! Observation is opt-in and retains agent-record provenance, not execution authority.
//!
//! It answers four kinds of request and nothing else:
//!   * the embedded UI (`build.rs`, `include_bytes!`) — no npm, no source tree;
//!   * `/fixtures/...` read from the export directory — real transcript content,
//!     which is why it never leaves `127.0.0.1`;
//!   * `/api/state` and `/api/refresh`, the two calls the Refresh action needs.
//!   * the guarded `/api/mind/...` shadow interpretation endpoints, backed only
//!     by the fixed loopback model and an in-memory cache.
//!
//! No tokio: one `std::thread` per connection, and a mutex so two Refresh
//! clicks cannot derive the corpus twice at once.
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use thruline_core::capture_exclusion::{ExclusionPolicy, LEFT_OUT_FIELD};
use thruline_core::ledger::Ledger;
use thruline_core::mind_grounded::{
    catalog_from_export, grounded_output_digest, FactCatalog, GroundedAnalysis, GroundedError,
    GroundedJobResult, GroundedOllamaClient, ModelIdentity, ReceiptHandle, CONTRACT_VERSION, MODEL,
    MODEL_DIGEST, PROMPT_VERSION, SCHEMA_VERSION,
};

use crate::export_all::{export_all_staging, Progress};

/// A line for whoever started HQ, if anyone is still reading.
///
/// `println!` panics when the pipe it writes to has no reader left, and HQ
/// outlives its reader: a script (or a test) that takes the address and stops
/// reading is enough. A panic in `run` ends the process and resets every
/// connection still waiting on its listener, so what HQ says to its terminal is
/// best-effort and answering requests is not.
macro_rules! say {
    () => {{
        let _ = writeln!(std::io::stdout());
    }};
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

mod capture_health;
mod capture_holds;
mod change_integrity;
mod decisions;
mod decisions_boundary;
mod done_templates;
mod first_use;
mod hq;
mod in_flight;
mod install_route;
mod integration_actions;
mod observation;
mod person_acts;
mod pr_preview;
mod preferences;
mod read_route;
mod review_projects;
mod run_permission_settings;
mod run_route;
mod weekly_summary;
use in_flight::InFlightGuard;
pub(crate) use run_route::probe_tool_for_prepare;

mod assets {
    include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));
}

/// The default preferred port. Nothing depends on the number; if it is taken the
/// server asks the OS for any free port and prints the one it actually got,
/// because a URL you have to guess is not a URL you can hand to a stranger.
const PREFERRED_PORT: u16 = 7777;

pub struct Options {
    pub observe_codex: Option<PathBuf>,
    pub verification: Option<(PathBuf, PathBuf)>,
    pub acceptance: Option<(PathBuf, PathBuf)>,
    pub db: PathBuf,
    pub dir: PathBuf,
    pub port: Option<u16>,
    pub open: bool,
    pub diagnostics: bool,
    /// Re-derive before serving even when an export is already on disk.
    pub refresh: bool,
    /// The static directory a UI track supplies, in place of the embedded app.
    /// `None` serves the embedded build. `Some` that does not resolve is a
    /// refusal to start — never a fallback and never a blank page.
    pub static_dir: Option<PathBuf>,
    /// The address to bind. Only loopback is accepted; anything else is refused
    /// before a socket exists. Exists so the refusal is reachable from the
    /// shipping binary rather than only from a test.
    pub address: Option<String>,
}

struct State {
    observer: Option<Arc<observation::Observer>>,
    verification: Option<LocalVerification>,
    shutting_down: Arc<AtomicBool>,
    acceptance: Option<(PathBuf, PathBuf)>,
    db: PathBuf,
    /// `THRULINE_HOME`, where the execution store (`exec.db`), the shim and
    /// the spool are kept when it is set. Usually unset, or the folder `db`
    /// is in; set apart from `HOME` it is a second folder, and `L1.1`'s figure
    /// has to count it too (T-7-1 rehearsal: the served number left `exec.db`
    /// out). `None` when it is not set.
    exec_home: Option<PathBuf>,
    dir: PathBuf,
    diagnostics: bool,
    /// Resolved, canonical, verified to contain an `index.html` before the
    /// server bound. `None` means the embedded UI.
    static_root: Option<PathBuf>,
    /// The `thruline` binary the JSON projections re-enter. Held rather than
    /// resolved per request so a failure to locate it stops the server at
    /// startup instead of turning into a 500 nobody expected.
    cli_exe: PathBuf,
    /// Held for the duration of a refresh. `try_lock` failing is the "already
    /// refreshing" answer, not an error.
    refreshing: Mutex<()>,
    busy: AtomicBool,
    /// Bounded count of store-hold refusals in this HQ process, served by
    /// `/api/hq/load` after the removal finishes.
    store_held_refusals: AtomicU64,
    /// The outcome of the last refresh, so the app can report what happened
    /// rather than only that something stopped happening.
    last_result: Mutex<Option<serde_json::Value>>,
    origin: String,
    mind: MindRuntime,
    /// P6-5 — the run route's per-launch token and its one-job-per-project table.
    run_route: run_route::RunRoute,
    /// P6-2 — the last complete read of each session, to say "read earlier,
    /// nothing changed since" instead of reading it again. Memory only.
    read_cache: read_route::ReadCache,
    /// HQ Next — the `/api/hq/` routes' index cache and load progress.
    hq: hq::HqState,
}

trait MindEngine: Send + Sync {
    fn probe(&self) -> std::result::Result<ModelIdentity, GroundedError>;
    fn analyze(
        &self,
        catalog: &FactCatalog,
    ) -> std::result::Result<GroundedAnalysis, GroundedError>;
}

impl MindEngine for GroundedOllamaClient {
    fn probe(&self) -> std::result::Result<ModelIdentity, GroundedError> {
        GroundedOllamaClient::probe(self)
    }

    fn analyze(
        &self,
        catalog: &FactCatalog,
    ) -> std::result::Result<GroundedAnalysis, GroundedError> {
        GroundedOllamaClient::analyze(self, catalog)
    }
}

struct MindRuntime {
    engine: Arc<dyn MindEngine>,
    in_flight: AtomicBool,
    cache: Mutex<MindCache>,
}

impl MindRuntime {
    fn production() -> Self {
        Self {
            engine: Arc::new(GroundedOllamaClient::new()),
            in_flight: AtomicBool::new(false),
            cache: Mutex::new(MindCache::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MindCacheKey {
    project: String,
    session: String,
    source_digest: String,
    exclusion_fingerprint: String,
    model: String,
    model_digest: String,
    contract: String,
    prompt: String,
    schema: String,
}

impl MindCacheKey {
    fn new(catalog: &FactCatalog, exclusion_fingerprint: &str) -> Self {
        Self {
            project: catalog.project().to_string(),
            session: catalog.session().to_string(),
            source_digest: catalog.source_digest().to_string(),
            exclusion_fingerprint: exclusion_fingerprint.to_string(),
            model: MODEL.to_string(),
            model_digest: MODEL_DIGEST.to_string(),
            contract: CONTRACT_VERSION.to_string(),
            prompt: PROMPT_VERSION.to_string(),
            schema: SCHEMA_VERSION.to_string(),
        }
    }
}

#[derive(Default)]
struct MindCache {
    entries: VecDeque<(MindCacheKey, GroundedAnalysis)>,
}

impl MindCache {
    const CAPACITY: usize = 16;

    fn get(&self, key: &MindCacheKey) -> Option<GroundedAnalysis> {
        self.entries
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, value)| value.clone())
    }

    fn insert(&mut self, key: MindCacheKey, value: GroundedAnalysis) {
        if let Some(index) = self.entries.iter().position(|(known, _)| known == &key) {
            self.entries.remove(index);
        }
        while self.entries.len() >= Self::CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((key, value));
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InterpretRequest {
    project: String,
    session: String,
    model: String,
}

#[derive(Default)]
struct RequestHeaders {
    host: String,
    origin: String,
    host_count: usize,
    origin_count: usize,
    content_lengths: Vec<String>,
    content_types: Vec<String>,
    transfer_encoding: bool,
    malformed: bool,
    /// P6-5 — every `X-ThruLine-Run-Token` header sent. Exactly one is accepted.
    run_tokens: Vec<String>,
    /// P6-5 — the browser's own statement of where a request came from.
    sec_fetch_site: Option<String>,
}

const MAX_REQUEST_LINE_BYTES: usize = 2 * 1024;
const MAX_HEADER_LINE_BYTES: usize = 8 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_HEADER_COUNT: usize = 64;

pub fn run(opts: Options) -> Result<()> {
    let mut snapshot_children = thruline_core::bounded_child::shutdown_guard();
    // Both refusals happen BEFORE anything is bound, exported or printed. A
    // server that starts and then discovers it has nothing to serve has already
    // shown the user a blank page.
    let address = resolve_bind_address(opts.address.as_deref())?;
    let static_root = resolve_static_root(opts.static_dir.as_deref())?;
    let cli_exe = std::env::current_exe().context(
        "locating this executable, which `/api/explain`, `/api/claims` and \
         `/api/sessions` re-enter to project the CLI's own shapes",
    )?;

    let listener = bind(address, opts.port)?;
    let addr = listener.local_addr().context("reading the bound address")?;
    let url = format!(
        "http://{addr}{}",
        if opts.verification.is_some() {
            "/verification#/trace"
        } else if opts.acceptance.is_some() {
            "/acceptance"
        } else {
            // T-2-7 — a machine with no project picked yet opens on the
            // first-use path instead of an Inbox that can only say it is
            // empty. The address is the app's own (`ui/src/routes.js`
            // `START_HASH`); the store is asked, never guessed.
            first_use::landing_hash(&opts.db)
        }
    );
    if let Some(root) = &static_root {
        say!(
            "Serving the UI from {} (not the embedded build).",
            root.display()
        );
    }

    std::fs::create_dir_all(&opts.dir)
        .with_context(|| format!("creating {}", opts.dir.display()))?;

    let index = opts.dir.join("index.json");
    let have_export = index.is_file();
    if opts.verification.is_some() {
        say!("Local verification workspace. Only accepted commands in the configured project can run.");
    } else if opts.acceptance.is_some() {
        say!("Showing accepted requirements. Checks run only through an explicit CLI command.");
    } else if opts.refresh || !have_export {
        if have_export {
            say!("Refreshing your projects (you asked for it with --refresh).");
        } else {
            say!("No reading of your projects yet — building one now.");
            say!("This reads the whole ledger once. It is the slow run; later ones reuse it.");
        }
        let report = run_export(&opts.db, &opts.dir, opts.diagnostics, true)?;
        say!(
            "Read {} projects in {}.",
            report.projects,
            human_duration(report.elapsed.as_secs_f64())
        );
    } else {
        match export_age(&index) {
            Some(age) => {
                say!("Showing the reading taken {age}. Refresh in the app to redo it.")
            }
            None => say!("Showing the last reading. Refresh in the app to redo it."),
        }
    }

    say!();
    say!("ThruLine is at {url}");
    say!("Press Ctrl+C to stop HQ and request cancellation of its checks.");

    if opts.open {
        open_browser(&url);
    }

    if opts.verification.is_some() {
        crate::verification_pack::verifier_identity()?;
    }
    let shutting_down = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    let signal_handlers = [
        signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutting_down))?,
        signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutting_down))?,
    ];
    let observer = if let Some(root) = opts.observe_codex {
        let (repo, db) = opts
            .verification
            .as_ref()
            .context("automatic observation requires a configured verification project")?;
        // C34 pause gate (5 of 6): the observer follows the pause record for
        // the folder it watches from its first cycle, before it reads anything.
        Some(observation::Observer::start(
            root,
            repo.clone(),
            db.clone(),
            cli_exe.clone(),
            Arc::clone(&shutting_down),
            Some(opts.db.clone()),
        )?)
    } else {
        None
    };
    let state = Arc::new(State {
        observer,
        shutting_down,
        verification: opts.verification.map(|(repo, db)| LocalVerification {
            repo,
            db,
            job: Mutex::new(LocalJob::default()),
        }),
        acceptance: opts.acceptance,
        db: opts.db,
        // Only an explicit `THRULINE_HOME` can put the execution store in a
        // folder of its own. Without it the store is `~/.thruline`, which is
        // this server's folder in the ordinary case — and when `--db` names
        // another ledger, `~/.thruline` holds a ledger this server does not
        // use, which is not what it is keeping.
        exec_home: std::env::var_os("THRULINE_HOME").map(PathBuf::from),
        dir: opts.dir,
        diagnostics: opts.diagnostics,
        static_root,
        cli_exe,
        refreshing: Mutex::new(()),
        busy: AtomicBool::new(false),
        store_held_refusals: AtomicU64::new(0),
        last_result: Mutex::new(None),
        origin: addr.to_string(),
        mind: MindRuntime::production(),
        run_route: run_route::RunRoute::new(),
        read_cache: read_route::ReadCache::new(),
        hq: hq::HqState::default(),
    });

    // T-3-4 (`Q12`) — HQ starting is the moment the record's open runs are
    // settled. Every `running` row whose process is gone becomes `interrupted`
    // ("HQ restarted while it ran"); every one whose pid is alive and is not
    // ours becomes `interrupted` with `pid reused`, and is NEVER adopted. A
    // run that really is ours and really is going is left exactly as it is.
    //
    // It runs before the first request is accepted, so no screen can ever
    // render a run as going when this process already knows it is not.
    reconcile_open_runs(&state.db);

    // HQ Next — build the most recently active project's index in the
    // background, so the first `/api/hq/` read does not pay for it.
    if state.verification.is_none() && state.acceptance.is_none() {
        hq::warm(&state);
    }

    listener.set_nonblocking(true)?;
    while !state.shutting_down.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                // A dead connection must never take the server with it: a panic
                // in one thread is one failed request, not a stopped product.
                std::thread::spawn(move || {
                    if let Err(err) = handle(&state, stream) {
                        // Browsers hang up mid-response constantly. Say it once,
                        // quietly, and keep serving.
                        tracing::debug!("request ended: {err}");
                    }
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => tracing::debug!("accept failed: {err}"),
        }
    }
    // ADV-run-security-5 — a question the run route is still putting to rustup
    // is killed with its process group, and none is asked after this, so no
    // question outlives HQ.
    state.run_route.stop_questions();
    if !snapshot_children.stop() {
        eprintln!("Snapshot child cleanup did not finish within 2 seconds; remaining processes were not verified absent.");
    }
    // T-3-4 (`B12.6`, `Q12`) — HQ was asked to stop, so it stops the checks it
    // owns FIRST, verifies nothing of theirs is left running, and records both
    // facts. Bounded: a wedged child must not stop the product exiting, and a
    // record that says "we looked and something remained" is worth more than a
    // product that hangs.
    cancel_owned_runs_for_shutdown(&state.db);

    // Stop accepting work before signalling owned checks. Allow the existing
    // collectors to write their incomplete finish and clean their children.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let local_running = state.verification.as_ref().is_some_and(|v| {
            v.job
                .lock()
                .map(|job| {
                    job.cancel.store(true, Ordering::SeqCst);
                    job.running
                })
                .unwrap_or(true)
        });
        let tools_running = integration_actions::cancel_for_shutdown();
        let observation_running = state
            .observer
            .as_ref()
            .is_some_and(|o| o.active.load(Ordering::SeqCst));
        if !local_running && !tools_running && !observation_running {
            break;
        }
        if std::time::Instant::now() >= deadline {
            eprintln!("Shutdown cleanup did not finish within 10 seconds. Interrupted checks remain incomplete; inspect remaining processes before retrying.");
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    #[cfg(unix)]
    for handler in signal_handlers {
        signal_hook::low_level::unregister(handler);
    }
    Ok(())
}

/// `POST /api/changes/<id>/runs/<run>/cancel` — one person stops one check.
///
/// The body carries a name and nothing else. There is no field in it that
/// reaches a verdict, a link, a scope or a decision, which is the same shape
/// discipline the decision and scope routes hold: the request says *who*, and
/// everything else is read off the record.
///
/// The run must belong to the Change in the path. A run key from another
/// Change is a 404 rather than a cancellation, so a route cannot be used to
/// reach across Changes by guessing keys.
fn change_cancel_run(
    state: &State,
    change_id: &str,
    run: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct Request {
        actor: Option<String>,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "malformed_request"}))?.as_slice(),
        );
    };
    let Some(actor) = request.actor.filter(|a| !a.trim().is_empty()) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "actor_required"}))?.as_slice(),
        );
    };
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    match thruline_core::run_execution::latest(&ledger, run)? {
        Some(record) if record.change_id.as_deref() == Some(change_id) => {}
        _ => {
            return respond(
                stream,
                404,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({"error": "no_such_run"}))?.as_slice(),
            )
        }
    }
    let policy = match change_action_policy(&ledger, change_id) {
        Ok(policy) => policy,
        Err(_) => return change_action_policy_unavailable(stream),
    };
    let acted = crate::run_supervisor::cancel(
        &ledger,
        run,
        &actor,
        thruline_core::run_execution::Reason::CancelledByPerson,
    )
    .context("stopping this check and looking at what it left")?;
    match acted {
        Err(refusal) => respond(
            stream,
            409,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": refusal.slug(),
                "plain": refusal.plain(),
            }))?
            .as_slice(),
        ),
        Ok(done) => {
            let (verified, survivors) = done.cleanup.as_record();
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
                "cancelled": true,
                "run": done.run_key,
                "name": done.name,
                "actor": actor,
                "cleanup_verified": verified,
                "survivors": survivors,
                "execution": thruline_core::run_execution::all(&ledger, Some(change_id))?,
            }))?;
            change_action_success(stream, &body, policy.as_ref())
        }
    }
}

/// T-3-4 — settle the record's open runs against the process table (`Q12`).
///
/// Every failure here is swallowed deliberately: HQ starting must not be
/// blocked by a reconciliation, and a pending schema upgrade is a refusal this
/// process already prints elsewhere (A193). Nothing is guessed — the liveness
/// answer is `run_supervisor`'s reading of `ps`, and a run it cannot classify
/// is recorded as interrupted rather than left looking alive.
fn reconcile_open_runs(db: &Path) {
    let Ok(ledger) = thruline_core::ledger::Ledger::open(db) else {
        return;
    };
    match thruline_core::run_execution::reconcile(
        &ledger,
        &thruline_core::ingest::iso8601_utc_now(),
        thruline_core::run_execution::Reason::HqRestarted,
        |supervisor| crate::run_supervisor::liveness(supervisor.pid, supervisor.process_group),
    ) {
        Ok(done) => {
            let settled = done.interrupted.len() + done.pid_reused.len();
            if settled > 0 {
                say!(
                    "{settled} check(s) were still marked as running from a previous session.                      They are recorded as interrupted; none of them counts as a pass."
                );
            }
        }
        Err(err) => tracing::debug!("open runs were not reconciled: {err}"),
    }
}

/// T-3-4 — stop what HQ owns before HQ exits, and say what was found.
///
/// The order is the one the requirement asks for: signal the run's process
/// group, wait a bounded time, LOOK at the process table, and record the
/// answer — including when the answer is "something remained". Cleanup is a
/// typed field on the row, never an assumption in a comment.
fn cancel_owned_runs_for_shutdown(db: &Path) {
    let Ok(ledger) = thruline_core::ledger::Ledger::open(db) else {
        return;
    };
    let Ok(open) = thruline_core::run_execution::running(&ledger) else {
        return;
    };
    for supervisor in open {
        let cleanup = match supervisor.process_group {
            Some(pgid) => {
                crate::run_supervisor::stop_group(pgid, crate::run_supervisor::STOP_BOUND)
            }
            None => crate::run_supervisor::Cleanup::Unknown,
        };
        let (verified, survivors) = cleanup.as_record();
        if let Err(err) = thruline_core::run_execution::finish(
            &ledger,
            &thruline_core::run_execution::Finish {
                run_key: supervisor.run_key.clone(),
                state: thruline_core::run_execution::ExecutionState::Cancelled,
                reason: Some(thruline_core::run_execution::Reason::HqShutDown),
                actor: None,
                binding_id: None,
                cleanup_verified: verified,
                survivors,
                at: thruline_core::ingest::iso8601_utc_now(),
            },
        ) {
            tracing::debug!("a shutdown cancellation was not recorded: {err}");
        }
    }
}

/// The only addresses this server will ever bind, and the refusal for the rest.
///
/// The invariant is CLAUDE.md's: this process hands out real transcript content,
/// so it must not be reachable from the network at all. That is normally stated
/// as "we bind loopback" — a promise nothing checks. Here it is a function with
/// a refusal in it, reachable from `--address`, so the guard that enforces the
/// invariant is a guard that ships.
///
/// `localhost` is accepted as a spelling of `127.0.0.1` and nothing else is
/// resolved: a hostname lookup is a network answer deciding where a
/// local-only server listens, which is the decision this function exists to
/// keep out of the network's hands.
fn resolve_bind_address(requested: Option<&str>) -> Result<Ipv4Addr> {
    let Some(requested) = requested else {
        return Ok(Ipv4Addr::LOCALHOST);
    };
    let trimmed = requested.trim();
    if trimmed.eq_ignore_ascii_case("localhost") {
        return Ok(Ipv4Addr::LOCALHOST);
    }
    match trimmed.parse::<Ipv4Addr>() {
        Ok(address) if address.is_loopback() => Ok(address),
        _ => Err(anyhow::anyhow!(
            "refusing to bind {requested}: `thruline ui` serves your transcripts and \
             answers only on the IPv4 loopback. Use 127.0.0.1 or localhost, or leave \
             `--address` off."
        )),
    }
}

fn bind(address: Ipv4Addr, preferred: Option<u16>) -> Result<TcpListener> {
    let want = preferred.unwrap_or(PREFERRED_PORT);
    let loopback = address;
    match TcpListener::bind(SocketAddr::from((loopback, want))) {
        Ok(listener) => Ok(listener),
        Err(err) if preferred.is_some() => Err(anyhow::anyhow!(
            "port {want} is not available ({err}). Pick another with `--port`, \
             or leave `--port` off and one will be chosen for you."
        )),
        Err(_) => TcpListener::bind(SocketAddr::from((loopback, 0)))
            .context("no loopback port available at all"),
    }
}

/// Validate the directory a UI track supplied, or refuse to start.
///
/// **A missing directory must fail loudly, not serve a blank page.** A UI that
/// renders nothing is indistinguishable from a UI reporting no findings, and
/// this product's whole thesis is that absence of a signal must not read as
/// absence of a problem. So every way of getting this wrong — absent, not a
/// directory, no `index.html` — stops the process with the path named, and
/// there is deliberately no fallback to the embedded build: a silent fallback
/// would show a working UI serving the wrong bytes, which is worse.
fn resolve_static_root(requested: Option<&Path>) -> Result<Option<PathBuf>> {
    let Some(requested) = requested else {
        return Ok(None);
    };
    let shown = requested.display();
    if !requested.exists() {
        anyhow::bail!(
            "no static directory at {shown}. `--static-dir` names the built UI to serve; \
             refusing to start and show a blank page instead. Leave `--static-dir` off to \
             serve the UI embedded in this binary."
        );
    }
    if !requested.is_dir() {
        anyhow::bail!("{shown} is not a directory. `--static-dir` needs the built UI's directory.");
    }
    let root = requested
        .canonicalize()
        .with_context(|| format!("resolving {shown}"))?;
    if !root.join("index.html").is_file() {
        anyhow::bail!(
            "{shown} has no index.html. A directory without an entry point serves a blank \
             page, and a blank page reads as a UI with no findings — refusing to start."
        );
    }
    Ok(Some(root))
}

fn run_export(
    db: &Path,
    dir: &Path,
    diagnostics: bool,
    print: bool,
) -> Result<crate::export_all::ExportAllReport> {
    // No pinned stamp here: a refresh a person asked for should say when it
    // was taken.
    export_all_staging(db, dir, diagnostics, None, &mut |p| {
        if !print {
            return;
        }
        match p {
            Progress::Starting { total } => {
                say!("  {total} projects to read.");
            }
            Progress::Project {
                done,
                total,
                id,
                elapsed,
            } => {
                // Every project, not a sampled subset: the point of the line is
                // that the user can see it is still moving.
                say!(
                    "  [{done}/{total}] {} · {}",
                    short_project(id),
                    human_duration(elapsed.as_secs_f64())
                );
                let _ = std::io::stdout().flush();
            }
        }
    })
}

/// The id between `prefix` and `suffix` in `path`: the empty id when the two
/// overlap with nothing between them. A path such as `/api/changes/decisions`
/// starts with `/api/changes/` and ends with `/decisions`, so the arm that
/// guards on both is chosen; slicing it by the suffix length was `[13..12]` and
/// the handler panicked. It now carries an empty id, which every handler
/// refuses as no such change (a 404), like `/api/changes//decisions`. The guards
/// keep their `starts_with` / `ends_with` shape on purpose: the licence census
/// and `docs-trace` read the routes from it.
fn path_between<'a>(path: &'a str, prefix: &str, suffix: &str) -> &'a str {
    path.strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .unwrap_or_default()
}

fn handle(state: &Arc<State>, mut stream: TcpStream) -> Result<()> {
    // macOS can inherit the listener's nonblocking mode on accept. This
    // request worker uses blocking, bounded I/O; otherwise write_all can stop
    // at WouldBlock after sending a valid header and only part of a receipt.
    stream.set_nonblocking(false)?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    if !stream.peer_addr().is_ok_and(peer_is_allowed) {
        return respond(
            &mut stream,
            403,
            "text/plain; charset=utf-8",
            b"loopback only",
        );
    }

    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .context("setting request read timeout")?;
    let mut reader = BufReader::new(stream.try_clone().context("cloning the connection")?);
    let mut line = String::new();
    let request_bytes = match read_bounded_line(&mut reader, &mut line, MAX_REQUEST_LINE_BYTES) {
        Ok(bytes) => bytes,
        Err(_) => {
            return respond(
                &mut stream,
                400,
                "text/plain; charset=utf-8",
                b"malformed request",
            )
        }
    };
    if request_bytes == 0 {
        return Ok(());
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 3 || parts[2] != "HTTP/1.1" {
        return respond(
            &mut stream,
            400,
            "text/plain; charset=utf-8",
            b"malformed request",
        );
    }
    let method = parts[0].to_string();
    let target = parts[1].to_string();

    let mut headers = RequestHeaders::default();
    let mut header_bytes = 0usize;
    let mut header_count = 0usize;
    loop {
        let mut header = String::new();
        let read = match read_bounded_line(&mut reader, &mut header, MAX_HEADER_LINE_BYTES) {
            Ok(read) => read,
            Err(_) => {
                return respond(
                    &mut stream,
                    400,
                    "text/plain; charset=utf-8",
                    b"malformed request",
                )
            }
        };
        if read == 0 {
            break;
        }
        header_bytes = header_bytes.saturating_add(read);
        header_count += 1;
        if header_bytes > MAX_HEADER_BYTES || header_count > MAX_HEADER_COUNT {
            return respond(
                &mut stream,
                400,
                "text/plain; charset=utf-8",
                b"malformed request",
            );
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.is_empty() || name.trim() != name {
                headers.malformed = true;
                continue;
            }
            match name.to_ascii_lowercase().as_str() {
                "host" => {
                    headers.host_count += 1;
                    headers.host = value.trim().to_string();
                }
                "origin" => {
                    headers.origin_count += 1;
                    headers.origin = value.trim().to_string();
                }
                "content-length" => headers.content_lengths.push(value.trim().to_string()),
                "content-type" => headers.content_types.push(value.trim().to_string()),
                "transfer-encoding" => headers.transfer_encoding = true,
                run_route::TOKEN_HEADER => headers.run_tokens.push(value.trim().to_string()),
                "sec-fetch-site" => headers.sec_fetch_site = Some(value.trim().to_string()),
                _ => {}
            }
        } else {
            headers.malformed = true;
        }
    }
    headers.malformed |= headers.host_count != 1 || headers.origin_count > 1;

    // This server hands out real transcript content. Binding to the loopback
    // stops the network; checking Host and Origin stops the *browser* — any page
    // the user has open could otherwise point a request at this port through a
    // hostname that resolves to 127.0.0.1 and read the answer.
    let host_ok = headers.host == state.origin
        || headers.host == format!("localhost:{}", port_of(&state.origin));
    let origin_ok = headers.origin.is_empty()
        || headers.origin == format!("http://{}", state.origin)
        || headers.origin == format!("http://localhost:{}", port_of(&state.origin));
    if !host_ok || !origin_ok {
        return respond(
            &mut stream,
            403,
            "text/plain; charset=utf-8",
            b"this address only answers to itself",
        );
    }

    let path = target.split('?').next().unwrap_or("/");
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    if path == "/api/observation" {
        if target.contains('?') || headers.malformed {
            return respond(
                &mut stream,
                400,
                "application/json",
                br#"{"error":"malformed_request"}"#,
            );
        }
        if method == "GET" {
            if !headers.content_lengths.is_empty()
                || !headers.content_types.is_empty()
                || headers.transfer_encoding
            {
                return respond(
                    &mut stream,
                    400,
                    "application/json",
                    br#"{"error":"malformed_request"}"#,
                );
            }
            let value=state.observer.as_ref().map(|o|o.read()).unwrap_or_else(||serde_json::json!({"enabled":false,"state":"not_configured","message":"Automatic observation is not configured. Start HQ with --observe-codex DIR for the chosen verification project."}));
            return respond(
                &mut stream,
                200,
                "application/json",
                &serde_json::to_vec(&value)?,
            );
        }
        if method == "POST" {
            if headers.origin_count != 1 {
                return respond(
                    &mut stream,
                    403,
                    "application/json",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
            };
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request {
                action: String,
            }
            let request = match serde_json::from_slice::<Request>(&body) {
                Ok(r) => r,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json",
                        br#"{"error":"invalid_observation_action"}"#,
                    )
                }
            };
            let Some(observer) = &state.observer else {
                return respond(
                    &mut stream,
                    409,
                    "application/json",
                    br#"{"error":"observation_not_configured"}"#,
                );
            };
            match request.action.as_str() {
                "pause" => observer.pause(true),
                // C34 pause gate (5 of 6): this switch cannot resume what the
                // pause record says is paused; resume it where it was paused.
                "resume" if observer.held_by_record() => {
                    return respond(
                        &mut stream,
                        409,
                        "application/json",
                        br#"{"error":"paused_in_record"}"#,
                    )
                }
                "resume" => observer.pause(false),
                _ => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json",
                        br#"{"error":"invalid_observation_action"}"#,
                    )
                }
            };
            return respond(
                &mut stream,
                200,
                "application/json",
                &serde_json::to_vec(&observer.read())?,
            );
        }
        return respond(
            &mut stream,
            405,
            "application/json",
            br#"{"error":"method_not_allowed"}"#,
        );
    }
    if path == "/api/verification" {
        if target.contains('?') || headers.malformed {
            return respond(
                &mut stream,
                400,
                "application/json",
                br#"{"error":"malformed_request"}"#,
            );
        }
        if method == "GET" {
            if !headers.content_lengths.is_empty()
                || !headers.content_types.is_empty()
                || headers.transfer_encoding
            {
                return respond(
                    &mut stream,
                    400,
                    "application/json",
                    br#"{"error":"malformed_request"}"#,
                );
            }
            return verification_read(state, &mut stream);
        }
        if method == "POST" {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
            };
            return verification_action(state, &body, &mut stream);
        }
        return respond(
            &mut stream,
            405,
            "application/json",
            br#"{"error":"method_not_allowed"}"#,
        );
    }
    if path == "/api/integrations/action" {
        if method != "POST" {
            return respond(
                &mut stream,
                405,
                "application/json",
                br#"{"error":"Use an explicit check action."}"#,
            );
        }
        if headers.origin_count != 1 || state.acceptance.is_some() || target.contains('?') {
            return respond(
                &mut stream,
                403,
                "application/json",
                br#"{"error":"Check actions require the local workspace origin."}"#,
            );
        }
        let body = match read_interpret_body(&mut reader, &headers) {
            Ok(body) => body,
            Err(_) => {
                return respond(
                    &mut stream,
                    400,
                    "application/json",
                    br#"{"error":"Invalid check request framing."}"#,
                )
            }
        };
        return integration_actions::handle(state, &body, &mut stream);
    }
    if matches!(
        path,
        "/api/integrations" | "/api/review-projects" | "/api/reviews/export"
    ) {
        if method != "GET" {
            return respond(&mut stream, 405, "text/plain", b"read-only endpoint");
        }
        if headers.malformed
            || headers.host_count != 1
            || headers.origin_count > 1
            || !headers.content_lengths.is_empty()
            || headers.transfer_encoding
            || !headers.content_types.is_empty()
        {
            return respond(
                &mut stream,
                400,
                "application/json",
                br#"{"error":"malformed_request"}"#,
            );
        }
        if state.acceptance.is_some() {
            return respond(
                &mut stream,
                404,
                "text/plain",
                b"not available in acceptance view",
            );
        }
        return match path {
            "/api/review-projects" if query.is_empty() => review_projects::list(state, &mut stream),
            "/api/review-projects" => respond(
                &mut stream,
                400,
                "application/json",
                br#"{"error":"unexpected_query"}"#,
            ),
            "/api/reviews/export" => review_projects::download(state, query, &mut stream),
            _ => integration_projection(state, query, &mut stream),
        };
    }
    if path == "/api/acceptance" {
        if method != "GET" {
            return respond(&mut stream, 405, "text/plain", b"read-only endpoint");
        }
        if headers.malformed
            || target.contains('?')
            || !headers.content_lengths.is_empty()
            || headers.transfer_encoding
            || !headers.content_types.is_empty()
        {
            return respond(&mut stream, 400, "text/plain", b"malformed request");
        }
        return acceptance_projection(state, &mut stream);
    }
    if state.acceptance.is_some() && (path.starts_with("/api/") || path.starts_with("/fixtures/")) {
        return respond(
            &mut stream,
            404,
            "text/plain",
            b"not available in acceptance view",
        );
    }
    if headers.malformed && matches!(path, "/api/mind/status" | "/api/mind/interpret") {
        return respond(
            &mut stream,
            400,
            "application/json; charset=utf-8",
            br#"{"error":"malformed_request"}"#,
        );
    }
    if target.contains('?') && matches!(path, "/api/mind/status" | "/api/mind/interpret") {
        return respond(
            &mut stream,
            400,
            "application/json; charset=utf-8",
            br#"{"error":"query_not_allowed"}"#,
        );
    }
    if method == "GET"
        && path == "/api/mind/status"
        && (!headers.content_lengths.is_empty()
            || !headers.content_types.is_empty()
            || headers.transfer_encoding)
    {
        return respond(
            &mut stream,
            400,
            "application/json; charset=utf-8",
            br#"{"error":"malformed_request"}"#,
        );
    }
    // A Change's confirmed session links, re-folded without writing metadata.
    if change_integrity::owns(path) {
        return change_integrity::handle(state, &method, path, query, &headers, &mut stream);
    }
    // HQ Next — the pull-request preview's branch, receipts and the CLI's
    // check-run name and badge words (C30 R-PR), read-only. Before the
    // one-Change arm, which would answer `/api/changes/<id>/pr` with 404.
    if pr_preview::owns(path) {
        return pr_preview::handle(state, &method, path, query, &headers, &mut stream);
    }
    // HQ Next — capture health (C30 R-ROUTES), read-only. Before `hq::owns`,
    // which claims every address under `/api/hq/projects/`.
    if capture_health::owns(path) {
        return capture_health::handle(state, &method, path, query, &headers, &mut stream);
    }
    // HQ Next — the templates a project's policy file holds for what counts
    // as done (C30 R-ROUTES), read-only. Before `hq::owns` for the same reason.
    if done_templates::owns(path) {
        return done_templates::handle(state, &method, path, query, &headers, &mut stream);
    }
    if decisions::owns(path) {
        // Decisions owns its read shapes and stored judgments. This scope is
        // only a final presentation rule for its project JSON replies.
        let _exclusion_boundary = decisions_boundary::covers(&method, path)
            .then(|| decisions_boundary::enter(&state.db, path));
        return decisions::handle(
            state,
            &method,
            path,
            query,
            &headers,
            &mut reader,
            &mut stream,
        );
    }
    // HQ Next — First run's Install (C30 R-ROUTES): the plan (GET) and the
    // one click that installs exactly what was ticked (POST, same-origin and
    // the run token). Before `hq::owns` for the same reason.
    if install_route::owns(path) {
        return install_route::handle(
            state,
            &method,
            path,
            query,
            &headers,
            &mut reader,
            &mut stream,
        );
    }
    // C46 Settings — permission rows by exact recorded worktree folder and
    // the guarded historical Take back address. Before the broad HQ router.
    if run_permission_settings::owns(path) {
        return run_permission_settings::handle(
            state,
            &method,
            path,
            query,
            &headers,
            &mut reader,
            &mut stream,
        );
    }
    // C51: aggregate record counts and opaque finding IDs for the open HQ page.
    if path == weekly_summary::PATH || path == weekly_summary::FINDINGS_PATH {
        return weekly_summary::handle(state, &method, path, query, &headers, &mut stream);
    }
    // HQ Next — every prompt, edit, command and claim of a project, read at
    // request time, and the one explicit load action (same-origin).
    if hq::owns(path) {
        return hq::handle(
            state,
            &method,
            path,
            query,
            &headers,
            &mut reader,
            &mut stream,
        );
    }
    // C30 R-PREFS — a person's preferences, in a file beside the ledger.
    if preferences::owns(path) {
        return preferences::handle(state, &method, query, &headers, &mut reader, &mut stream);
    }
    // P6-5 — the run route owns its four addresses whole, including the
    // same-origin and per-launch-token guards on its two writes.
    if run_route::owns(path) {
        return run_route::handle(
            state,
            &method,
            path,
            query,
            &headers,
            &mut reader,
            &mut stream,
        );
    }
    match (method.as_str(), path) {
        ("GET", "/api/mind/status") => mind_status(state, &mut stream),
        ("POST", "/api/mind/interpret") => {
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(BodyProblem::BadRequest) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
                Err(BodyProblem::TooLarge) => {
                    return respond(
                        &mut stream,
                        413,
                        "application/json; charset=utf-8",
                        br#"{"error":"request_too_large"}"#,
                    )
                }
            };
            mind_interpret(state, &body, &mut stream)
        }
        (_, "/api/mind/status" | "/api/mind/interpret") => respond(
            &mut stream,
            405,
            "text/plain; charset=utf-8",
            b"method not allowed",
        ),
        ("GET", "/api/lanes") => {
            // RA6 — live agent-lane presence, derived AT READ from vendor
            // recording mtimes. No watcher: each poll is one directory scan
            // that lives inside this request (M6.R4), and the whole surface
            // dies with the process like everything else here.
            let body = serde_json::to_vec(&lanes_json())?;
            respond(&mut stream, 200, "application/json; charset=utf-8", &body)
        }
        ("GET", "/api/storage") => storage(state, query, &mut stream),
        // T-DIST-4 — whether this machine has a licence that works, and the one
        // paid surface. A read, and the only licence route: there is no upload
        // and no write address, because a licence is installed from a terminal
        // (`thruline licence install <file>`). The same document
        // `thruline licence status --json` prints, from the same function.
        ("GET", "/api/licence") => {
            let body = serde_json::to_vec(&crate::json_out::envelope(
                &crate::json_out::LICENCE_STATUS,
                crate::licence_cmd::status_body(),
            )?)?;
            respond(&mut stream, 200, "application/json; charset=utf-8", &body)
        }
        (_, "/api/licence") => respond(
            &mut stream,
            405,
            "text/plain; charset=utf-8",
            b"method not allowed",
        ),
        // T-2-7 — the first-use path. One read of what this machine already
        // has, one read of the Change that work would make, and two writes
        // behind the same same-origin requirement every other action route
        // here carries. None of them accepts a path, a command or a sentence
        // from the browser.
        ("GET", "/api/first-use") => first_use::overview(state, query, &mut stream),
        ("GET", "/api/first-use/proposal") => first_use::proposal(state, query, &mut stream),
        ("POST", p @ ("/api/first-use/register" | "/api/first-use/change")) => {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
            };
            if p.ends_with("/register") {
                first_use::register(state, &body, &mut stream)
            } else {
                first_use::accept(state, &body, &mut stream)
            }
        }
        (
            _,
            "/api/first-use"
            | "/api/first-use/proposal"
            | "/api/first-use/register"
            | "/api/first-use/change",
        ) => respond(
            &mut stream,
            405,
            "text/plain; charset=utf-8",
            b"method not allowed",
        ),
        ("GET", "/api/state") => {
            let body = serde_json::to_vec(&state_json(state))?;
            respond(&mut stream, 200, "application/json; charset=utf-8", &body)
        }
        ("POST", "/api/refresh") => refresh(state, &mut stream),
        ("GET", "/api/explain") => projection(state, "explain", query, &mut stream),
        ("GET", "/api/claims") => projection(state, "claims", query, &mut stream),
        ("GET", "/api/phases") => phases(state, query, &mut stream),
        // P6-2 — the Read: the steps of opening one piece of work's record.
        ("GET", "/api/read") => read_route::read(state, query, &mut stream),
        // The list a session is CHOSEN from. `/api/explain` with no `session`
        // opens whichever id sorts last, which on a real store is an arbitrary
        // session with nothing in it — that default is untouched, and this is
        // how a person reaches the others.
        ("GET", "/api/sessions") => {
            // No parameter is accepted, so none is silently ignored: a `?limit=`
            // that did nothing would be a UI showing 50 rows and believing it
            // asked for 200. Refused before a process is spawned.
            if !query.is_empty() {
                return respond(
                    &mut stream,
                    400,
                    "application/json; charset=utf-8",
                    serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                        .as_slice(),
                );
            }
            spawn_projection(state, "sessions", &["--per-project"], &mut stream)
        }
        // T-2-2 — the one write route in the Change layer. It appends a
        // person's decision about one link and can produce nothing else: no
        // Change, no scope, no criteria, no result. Same-origin required, like
        // every other action route here.
        ("POST", p) if p.starts_with("/api/changes/") && p.ends_with("/associations") => {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
            };
            let id = path_between(p, "/api/changes/", "/associations").to_string();
            change_association(state, &id, &body, &mut stream)
        }
        // T-S-4 — the two decision routes, and there are exactly two. Each
        // appends ONE row naming one version, one actor, one time and one
        // accepted-criteria version. Neither merges, pushes, deploys, writes to
        // a branch, or sends anything anywhere; the only file either touches is
        // `.thruline/decisions/<version>.json` under the project's own
        // worktree. Same-origin required, like every other action route here.
        //
        // `/withdraw` is matched FIRST because the approve arm below matches
        // the prefix `/decisions` and would otherwise swallow it.
        ("POST", p) if p.starts_with("/api/changes/") && p.ends_with("/decisions/withdraw") => {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let Some(body) = person_acts::read_body(&mut reader, &headers, &mut stream)? else {
                return Ok(());
            };
            let id = path_between(p, "/api/changes/", "/decisions/withdraw").to_string();
            change_decide(state, &id, &body, false, &mut stream)
        }
        ("POST", p) if p.starts_with("/api/changes/") && p.ends_with("/decisions") => {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let Some(body) = person_acts::read_body(&mut reader, &headers, &mut stream)? else {
                return Ok(());
            };
            let id = path_between(p, "/api/changes/", "/decisions").to_string();
            change_decide(state, &id, &body, true, &mut stream)
        }
        // C30 R-LEDGER — a person keeps one note on a Change. It appends ONE
        // `change_notes` row with no decision and can produce nothing else: no
        // verdict, count, readiness or approval reads it, and it is free (the
        // one paid surface is approving). Same-origin required, like every
        // other action route here. The Approve note does not come this way: it
        // rides on `…/decisions` and is appended by the act itself.
        ("POST", p) if p.starts_with("/api/changes/") && p.ends_with("/notes") => {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let Some(body) = person_acts::read_body(&mut reader, &headers, &mut stream)? else {
                return Ok(());
            };
            let id = path_between(p, "/api/changes/", "/notes").to_string();
            person_acts::add_note(state, &id, &body, &mut stream)
        }
        // C30 R-LEDGER — a second person puts their name on the version the
        // standing approval names (row 22), or takes their own second back
        // (row 23). One `change_decision_seconds` row each, free (C34 Q2): the
        // paid act is approving, and a second never makes an approval stand,
        // never rewrites the decision file and never moves a result. The longer
        // address is matched first. Same-origin required.
        ("POST", p)
            if p.starts_with("/api/changes/")
                && (p.ends_with("/seconds/withdraw") || p.ends_with("/seconds")) =>
        {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let (stem, withdrawing) = match p.strip_suffix("/seconds/withdraw") {
                Some(stem) => (stem, true),
                None => (p.strip_suffix("/seconds").unwrap_or(p), false),
            };
            let Some(id) = stem
                .strip_prefix("/api/changes/")
                .filter(|id| !id.is_empty())
                .map(str::to_string)
            else {
                return respond(
                    &mut stream,
                    404,
                    "application/json; charset=utf-8",
                    br#"{"error":"no_such_change"}"#,
                );
            };
            let Some(body) = person_acts::read_body_capped(
                &mut reader,
                &headers,
                &mut stream,
                person_acts::SECONDS_BODY_MAX,
            )?
            else {
                return Ok(());
            };
            if withdrawing {
                person_acts::withdraw_second(state, &id, &body, &mut stream)
            } else {
                person_acts::add_second(state, &id, &body, &mut stream)
            }
        }
        // T-5-3 — the fifth and sixth write addresses (`B7.18`-`B7.22`). Each
        // appends ONE `change_obligation_waivers` row saying a named person, under
        // the authority the ACCEPTED profile records, set one obligation aside
        // with a reason. Neither reaches a verdict, a link, a scope, a run or a
        // decision; neither can write a result; and the three prerequisites of
        // `B7.12` are refused by `ObligationWaiver::validate` and by a CHECK on the
        // table, so no request shape reaches them.
        //
        // `/not-applicable` is matched FIRST for the same reason `/withdraw`
        // is above: a suffix match on the shorter path would swallow it.
        ("POST", p)
            if p.starts_with("/api/changes/")
                && p.contains("/obligations/")
                && p.ends_with("/not-applicable") =>
        {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
            };
            let Some((id, obligation)) = obligation_address(p, "/not-applicable") else {
                return respond(
                    &mut stream,
                    404,
                    "application/json; charset=utf-8",
                    br#"{"error":"no_such_change"}"#,
                );
            };
            change_waive(
                state,
                &id,
                &obligation,
                thruline_core::change_readiness::WaiverKind::NotApplicable,
                &body,
                &mut stream,
            )
        }
        ("POST", p)
            if p.starts_with("/api/changes/")
                && p.contains("/obligations/")
                && p.ends_with("/waive") =>
        {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
            };
            let Some((id, obligation)) = obligation_address(p, "/waive") else {
                return respond(
                    &mut stream,
                    404,
                    "application/json; charset=utf-8",
                    br#"{"error":"no_such_change"}"#,
                );
            };
            change_waive(
                state,
                &id,
                &obligation,
                thruline_core::change_readiness::WaiverKind::Waived,
                &body,
                &mut stream,
            )
        }
        // T-2-3 — the one write route for what counts as done. It appends one
        // scope version and one acceptance record and can produce nothing else:
        // no decision, no approval, no result. Same-origin required.
        ("POST", p) if p.starts_with("/api/changes/") && p.ends_with("/scope/accept") => {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            // C30: its cap is sized from the parts a person can type (the
            // lines, the out-of-scope items), not the 512 bytes the other
            // action routes keep.
            let body = match read_json_body(&mut reader, &headers, SCOPE_ACCEPT_BODY_MAX) {
                Ok(body) => body,
                Err(BodyProblem::BadRequest) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
                Err(BodyProblem::TooLarge) => {
                    return respond(
                        &mut stream,
                        413,
                        "application/json; charset=utf-8",
                        br#"{"error":"request_too_large"}"#,
                    )
                }
            };
            let id = path_between(p, "/api/changes/", "/scope/accept").to_string();
            change_scope_accept(state, &id, &body, &mut stream)
        }
        // T-3-4 — the one write route that stops a running check. It can
        // append exactly one `cancelled` execution record and can reach
        // nothing else: no verdict, no link, no scope, no decision. Same-origin
        // required, like every other action route here.
        ("POST", p)
            if p.starts_with("/api/changes/") && p.ends_with("/cancel") && p.contains("/runs/") =>
        {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
            };
            let rest = path_between(p, "/api/changes/", "/cancel");
            let Some((change_id, run)) = rest.split_once("/runs/") else {
                return respond(
                    &mut stream,
                    404,
                    "application/json; charset=utf-8",
                    br#"{"error":"no_such_route"}"#,
                );
            };
            let (change_id, run) = (change_id.to_string(), run.to_string());
            change_cancel_run(state, &change_id, &run, &body, &mut stream)
        }
        // T-4-2 — the one write route for where one thing a review found
        // stands. It appends one row to `change_review_finding_standings`
        // through `finding_standing::record` — the function the terminal door
        // `thruline change finding` calls — and reaches nothing else: no
        // verdict, no link, no scope, no decision. Same-origin required, like
        // every other action route here.
        ("POST", p)
            if p.starts_with("/api/changes/")
                && p.ends_with("/standing")
                && p.contains("/findings/") =>
        {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            // Its body cap is the one limit both doors hold, not the 512
            // bytes the other action routes keep (ADV-review-input-2).
            let body = match read_json_body(&mut reader, &headers, STANDING_BODY_MAX) {
                Ok(body) => body,
                Err(BodyProblem::BadRequest) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
                Err(BodyProblem::TooLarge) => {
                    return respond(
                        &mut stream,
                        413,
                        "application/json; charset=utf-8",
                        br#"{"error":"too_long"}"#,
                    )
                }
            };
            let rest = path_between(p, "/api/changes/", "/standing");
            let Some((change_id, finding)) = rest.split_once("/findings/") else {
                return respond(
                    &mut stream,
                    404,
                    "application/json; charset=utf-8",
                    br#"{"error":"no_such_route"}"#,
                );
            };
            let (change_id, finding) = (change_id.to_string(), finding.to_string());
            change_finding_standing(state, &change_id, &finding, &body, &mut stream)
        }
        // T-3-1 — the one write route for what bears on a thing that counts as
        // done. It appends one link row or one tombstone and can produce
        // nothing else: no scope, no criteria, no result, and it never deletes.
        // Same-origin required, like every other action route here.
        ("POST", p) if p.starts_with("/api/changes/") && p.ends_with("/links") => {
            if headers.origin.is_empty() {
                return respond(
                    &mut stream,
                    403,
                    "application/json; charset=utf-8",
                    br#"{"error":"same_origin_required"}"#,
                );
            }
            let body = match read_interpret_body(&mut reader, &headers) {
                Ok(body) => body,
                Err(_) => {
                    return respond(
                        &mut stream,
                        400,
                        "application/json; charset=utf-8",
                        br#"{"error":"malformed_request"}"#,
                    )
                }
            };
            let id = path_between(p, "/api/changes/", "/links").to_string();
            change_links(state, &id, &body, &mut stream)
        }
        // T-4-2 — every finding of this Change's reviews and where each one
        // stands. Read-only; matched before the one-Change arm.
        ("GET", p) if p.starts_with("/api/changes/") && p.ends_with("/findings") => {
            let id = path_between(p, "/api/changes/", "/findings");
            change_findings(state, id, query, &mut stream)
        }
        // T-3-5 — whether this version is ready for approval, and what that
        // rests on. Read-only; there is no write address under this path.
        ("GET", p) if p.starts_with("/api/changes/") && p.ends_with("/readiness") => {
            let id = path_between(p, "/api/changes/", "/readiness");
            change_readiness_route(state, id, query, &mut stream)
        }
        // T-3-1 — what the record holds for each thing that counts as done.
        // Read-only, and matched before the one-Change arm for the same reason
        // the proposals arm is.
        ("GET", p) if p.starts_with("/api/changes/") && p.ends_with("/coverage") => {
            let id = path_between(p, "/api/changes/", "/coverage");
            change_coverage(state, id, query, &mut stream)
        }
        // T-S-2 — whether a piece of linked evidence still applies once the
        // tree it was checked on has moved. Read-only: this route derives
        // nothing and writes nothing; a record only exists once something
        // called `evidence_applicability::assess_tree_move` or
        // `reassess_for_scope_change`. Matched before the one-Change arm for
        // the same reason `/coverage` is.
        ("GET", p) if p.starts_with("/api/changes/") && p.ends_with("/applicability") => {
            let id = path_between(p, "/api/changes/", "/applicability");
            applicability_of_change(state, id, query, &mut stream)
        }
        // T-4-1 — one review's raw report or parsed record, one click below
        // the Change screen. Read-only, and served only as the masked
        // inspection copy `change_review::inspect_*` builds; the original
        // bytes never leave the store. Matched before the one-Change arm.
        ("GET", p)
            if p.starts_with("/api/changes/")
                && p.contains("/reviews/")
                && (p.ends_with("/raw") || p.ends_with("/parsed")) =>
        {
            change_review_artifact(state, p, query, &mut stream)
        }
        // T-2-3 — the proposals, read-only. Matched before the one-Change arm
        // below, which would otherwise swallow the suffix and answer 404 for a
        // Change that exists.
        ("GET", p) if p.starts_with("/api/changes/") && p.ends_with("/inputs") => {
            let id = path_between(p, "/api/changes/", "/inputs");
            change_inputs(state, id, query, &mut stream)
        }
        ("GET", p) if p.starts_with("/api/changes/") && p.ends_with("/proposals") => {
            let id = path_between(p, "/api/changes/", "/proposals");
            change_proposals(state, id, query, &mut stream)
        }
        // T-5-4 — whether the records the taken-away copy rests on can be
        // read now. Read-only: the file itself is made in the browser from the
        // previewed string, and nothing under this path writes.
        ("GET", p) if p.starts_with("/api/changes/") && p.ends_with("/export") => {
            let id = path_between(p, "/api/changes/", "/export");
            change_export_records(state, id, query, &mut stream)
        }
        // T-2-1 — the Change records, read-only.
        ("GET", "/api/changes") => changes_index(state, query, &mut stream),
        ("GET", p) if p.starts_with("/api/changes/") => {
            let id = &p["/api/changes/".len()..];
            changes_one(state, id, query, &mut stream)
        }
        // T-2-2 — a session's read-only view gains one field: the Changes it
        // is linked to, with provenance, or the sentence that says none.
        // `/api/sessions` itself is untouched, byte for byte.
        ("GET", p) if p.starts_with("/api/sessions/") && p.ends_with("/changes") => {
            let id = path_between(p, "/api/sessions/", "/changes");
            session_changes(state, id, query, &mut stream)
        }
        (_, "/api/changes") => respond(
            &mut stream,
            405,
            "text/plain; charset=utf-8",
            b"method not allowed",
        ),
        ("GET", p) if p.starts_with("/fixtures/") => serve_fixture(state, p, &mut stream),
        ("GET", p) => serve_asset(state, p, &mut stream),
        _ => respond(
            &mut stream,
            405,
            "text/plain; charset=utf-8",
            b"method not allowed",
        ),
    }
}

/// Re-enter the installed executable, with no browser-selected paths or commands.
/// One request at a time, 30 seconds and bounded streams; never serve an old green
/// result after a failed refresh. No transcript export or check execution occurs.
fn acceptance_projection(state: &State, stream: &mut TcpStream) -> Result<()> {
    let Some((repo, db)) = &state.acceptance else {
        return respond(
            stream,
            404,
            "text/plain",
            b"start ui with --acceptance REPO",
        );
    };
    let Ok(_guard) = state.refreshing.try_lock() else {
        return respond(
            stream,
            503,
            "text/plain",
            b"a reading is already in progress",
        );
    };
    let policy = match integration_policy_for_session(state, "", Some(repo)) {
        Ok(policy) => policy,
        Err(problem) if error_is_store_held(&problem) => return store_held(state, stream),
        Err(_) => return change_action_policy_unavailable(stream),
    };
    let result = (|| -> Result<Vec<u8>> {
        let mut command = std::process::Command::new(&state.cli_exe);
        command
            .args(["acceptance", "status", "--json", "--repo"])
            .arg(repo)
            .arg("--exec-db")
            .arg(db);
        let bytes = structured_projection(command)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            value["schema_version"] == 1 && value["state"].is_string(),
            "unknown status schema"
        );
        Ok(bytes)
    })();
    match result {
        Ok(bytes) => match present_acceptance_status(&bytes, &policy) {
            Ok(served) => respond(stream, 200, "application/json; charset=utf-8", &served),
            Err(_) => change_action_policy_unavailable(stream),
        },
        Err(error) if error_is_store_held(&error) => store_held(state, stream),
        Err(error) => {
            eprintln!(
                "acceptance reading: {}",
                thruline_capture::redaction::Redactor::new()
                    .redact(&error.to_string())
                    .bounded(240)
                    .text()
            );
            respond(stream, 502, "application/json; charset=utf-8",
                br#"{"error":"ThruLine could not read your acceptance status. Run acceptance status in the terminal to inspect the cause; no previous result is being shown."}"#)
        }
    }
}

fn present_acceptance_status<'a>(
    body: &'a [u8],
    policy: &ExclusionPolicy,
) -> Result<Cow<'a, [u8]>> {
    crate::exclusion_serve::bytes(body, policy)
}

#[cfg(test)]
mod acceptance_exclusion_tests {
    use super::*;
    use thruline_core::capture_hold::ExclusionView;

    #[test]
    fn status_hides_an_excluded_objective_and_preserves_a_miss() {
        let policy = ExclusionPolicy::from_exclusions(
            "/synthetic/repo",
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
        .expect("synthetic policy");
        let raw = br#"{"schema_version":1,"state":"reviewed","objective":"Review private/secret.rs","checks":[{"file":"private/secret.rs","reason":"pathless private detail"}]}"#;
        let served = present_acceptance_status(raw, &policy).expect("filtered status");
        let text = String::from_utf8(served.into_owned()).expect("status UTF-8");
        assert!(!text.contains("private/secret.rs"));
        assert!(!text.contains("pathless private detail"));
        assert!(text.contains("left out by your exclusion"));
        let unrelated =
            br#"{"schema_version":1,"state":"ready","objective":"Review src/public.rs"}"#;
        assert!(
            matches!(present_acceptance_status(unrelated, &policy), Ok(Cow::Borrowed(bytes)) if bytes == unrelated)
        );
    }
}

/// The installed CLI's local JSON is structured evidence, not transcript text.
/// Generic entropy redaction would destroy its identity digests. Keep the bytes
/// local and unchanged, bound the child and both read/size limits. Stderr is
/// drained without retaining it except for an exact fixed hold signal.
#[derive(Debug)]
struct ChildStoreHeld;

impl std::fmt::Display for ChildStoreHeld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThruLine is removing a project's records; try again when it finishes")
    }
}

impl std::error::Error for ChildStoreHeld {}

#[cfg(unix)]
fn structured_projection(mut command: std::process::Command) -> Result<Vec<u8>> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::time::Instant;
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-KILL", &format!("-{}", self.0.id())])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Child(
        command
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let stdout = child.0.stdout.take().context("missing projection stdout")?;
    let mut stderr = child.0.stderr.take().context("missing projection stderr")?;
    let (tx, rx) = std::sync::mpsc::channel();
    const LIMIT: usize = 1024 * 1024;
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = tx.send(result);
    });
    let (stderr_tx, stderr_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let expected = b"ThruLine is removing a project's records; try again when it finishes\n";
        let mut matched = true;
        let mut seen = 0usize;
        let mut chunk = [0u8; 4096];
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    matched &= seen.checked_add(n).is_some_and(|end| {
                        end <= expected.len() && expected[seen..end] == chunk[..n]
                    });
                    seen = seen.saturating_add(n);
                }
                Err(_) => {
                    matched = false;
                    break;
                }
            }
        }
        let _ = stderr_tx.send(matched && seen == expected.len());
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut bytes = None;
    loop {
        if let Ok(result) = rx.try_recv() {
            let value = result?;
            anyhow::ensure!(value.len() <= LIMIT, "status exceeded 1 MiB");
            bytes = Some(value);
        }
        if let Some(status) = child.0.try_wait()? {
            let held = stderr_rx
                .recv_timeout(Duration::from_millis(500))
                .unwrap_or(false);
            if status.code() == Some(3) && held {
                return Err(ChildStoreHeld.into());
            }
            anyhow::ensure!(status.success(), "status command failed");
            let value = match bytes {
                Some(bytes) => bytes,
                None => rx
                    .recv_timeout(Duration::from_millis(500))
                    .context("status output did not close")??,
            };
            anyhow::ensure!(value.len() <= LIMIT, "status exceeded 1 MiB");
            return Ok(value);
        }
        anyhow::ensure!(Instant::now() < deadline, "status exceeded 30 seconds");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(not(unix))]
fn structured_projection(_command: std::process::Command) -> Result<Vec<u8>> {
    anyhow::bail!("bounded acceptance projection requires Unix process groups on this build")
}

fn peer_is_allowed(address: SocketAddr) -> bool {
    address.ip().is_loopback()
}

fn read_bounded_line(
    reader: &mut BufReader<TcpStream>,
    line: &mut String,
    max: usize,
) -> std::io::Result<usize> {
    let read = {
        let mut bounded = (&mut *reader).take((max + 1) as u64);
        bounded.read_line(line)?
    };
    if read > max || (read > 0 && !line.ends_with("\r\n")) {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "HTTP line exceeded its bound or lacked CRLF",
        ));
    }
    Ok(read)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyProblem {
    BadRequest,
    TooLarge,
}

/// The body cap every action route keeps, except the standing route and the
/// three that can carry a note (`person_acts::BODY_MAX`).
const ACTION_BODY_MAX: usize = 512;

fn read_interpret_body(
    reader: &mut BufReader<TcpStream>,
    headers: &RequestHeaders,
) -> std::result::Result<Vec<u8>, BodyProblem> {
    read_json_body(reader, headers, ACTION_BODY_MAX)
}

/// One JSON body of at most `limit` bytes, framed exactly as
/// [`read_interpret_body`] requires.
fn read_json_body(
    reader: &mut BufReader<TcpStream>,
    headers: &RequestHeaders,
    limit: usize,
) -> std::result::Result<Vec<u8>, BodyProblem> {
    if headers.malformed
        || headers.transfer_encoding
        || headers.content_lengths.len() != 1
        || headers.content_types.len() != 1
        || !json_media_type(&headers.content_types[0])
    {
        return Err(BodyProblem::BadRequest);
    }
    let length = &headers.content_lengths[0];
    if length.is_empty() || !length.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(BodyProblem::BadRequest);
    }
    let length: usize = length.parse().map_err(|_| BodyProblem::BadRequest)?;
    if length > limit {
        return Err(BodyProblem::TooLarge);
    }
    if length == 0 {
        return Err(BodyProblem::BadRequest);
    }
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|_| BodyProblem::BadRequest)?;
    if !reader.buffer().is_empty() {
        return Err(BodyProblem::BadRequest);
    }
    reader
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(25)))
        .map_err(|_| BodyProblem::BadRequest)?;
    let mut extra = [0u8; 1];
    match reader.read(&mut extra) {
        Ok(0) => {}
        Ok(_) => return Err(BodyProblem::BadRequest),
        Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
        Err(_) => return Err(BodyProblem::BadRequest),
    }
    Ok(body)
}

fn json_media_type(value: &str) -> bool {
    let mut parts = value.split(';');
    if !parts
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
    {
        return false;
    }
    let mut saw_charset = false;
    parts.all(|parameter| {
        if saw_charset {
            return false;
        }
        let Some((name, value)) = parameter.trim().split_once('=') else {
            return false;
        };
        let valid = name.trim().eq_ignore_ascii_case("charset")
            && value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8");
        saw_charset = valid;
        valid
    })
}

fn mind_status(state: &State, stream: &mut TcpStream) -> Result<()> {
    let identity = state
        .mind
        .engine
        .probe()
        .ok()
        .filter(|identity| identity.tag == MODEL && identity.digest == MODEL_DIGEST);
    let body = serde_json::to_vec(&serde_json::json!({
        "available": identity.is_some(),
        "model": MODEL,
        "model_digest": identity.as_ref().map(|model| model.digest.as_str()),
        "provider": "ollama",
        "contract_version": CONTRACT_VERSION,
        "prompt_version": PROMPT_VERSION,
        "schema_version": SCHEMA_VERSION,
        "tier": "language",
        "mode": "shadow",
        "truth_bearing": false,
    }))?;
    respond(stream, 200, "application/json; charset=utf-8", &body)
}

fn mind_interpret(state: &State, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    let request: InterpretRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => {
            return respond(
                stream,
                400,
                "application/json; charset=utf-8",
                br#"{"error":"malformed_json"}"#,
            )
        }
    };
    if !valid_fixture_id(&request.project) || request.model != MODEL {
        return semantic_rejection(stream);
    }
    let fixture = state
        .dir
        .join("projects")
        .join(format!("{}.json", request.project));
    let raw = match std::fs::read(fixture) {
        Ok(bytes) => bytes,
        Err(_) => return semantic_rejection(stream),
    };
    // Keep a malformed or wrong-session fixture a semantic 422 before the
    // policy read. This validation stays inside HQ; only the filtered bytes
    // below become the catalog passed to the local model.
    if catalog_from_export(&raw, &request.project, &request.session).is_err() {
        return semantic_rejection(stream);
    }
    let ledger = match Ledger::open(&state.db) {
        Ok(ledger) => ledger,
        Err(thruline_core::error::CoreError::StoreHeld { .. }) => return store_held(state, stream),
        Err(_) => return internal_mind_error(stream),
    };
    let (bytes, exclusion_fingerprint) = match present_project_fixture(&ledger, &raw) {
        Ok(presented) => presented,
        Err(_) => return internal_mind_error(stream),
    };
    let catalog = match catalog_from_export(bytes.as_ref(), &request.project, &request.session) {
        Ok(catalog) => catalog,
        Err(_) => return semantic_rejection(stream),
    };
    let key = MindCacheKey::new(&catalog, &exclusion_fingerprint);
    let cached = match state.mind.cache.lock() {
        Ok(cache) => cache.get(&key),
        Err(_) => return internal_mind_error(stream),
    };
    if let Some(analysis) = cached {
        return mind_response(stream, true, &analysis);
    }

    let Some(_in_flight) = InFlightGuard::acquire(&state.mind.in_flight) else {
        return respond(
            stream,
            409,
            "application/json; charset=utf-8",
            br#"{"error":"analysis_in_progress"}"#,
        );
    };
    let analysis = match state.mind.engine.analyze(&catalog) {
        Ok(analysis) if complete_analysis(&analysis, &catalog, bytes.as_ref()) => analysis,
        Ok(_) | Err(_) => {
            return respond(
                stream,
                503,
                "application/json; charset=utf-8",
                br#"{"error":"local_model_unavailable"}"#,
            )
        }
    };
    match state.mind.cache.lock() {
        Ok(mut cache) => cache.insert(key, analysis.clone()),
        Err(_) => return internal_mind_error(stream),
    }
    mind_response(stream, false, &analysis)
}

fn valid_fixture_id(value: &str) -> bool {
    value.len() == 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn complete_analysis(analysis: &GroundedAnalysis, catalog: &FactCatalog, source: &[u8]) -> bool {
    let expected_jobs = [
        thruline_core::mind::MindJob::SessionNarration,
        thruline_core::mind::MindJob::DecisionProposal,
    ];
    let root: serde_json::Value = match serde_json::from_slice(source) {
        Ok(root) => root,
        Err(_) => return false,
    };
    let known_nodes: BTreeSet<&str> = root
        .get("receipts")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|receipts| {
            receipts.iter().filter_map(|(node, receipt)| {
                (receipt
                    .get("claim")
                    .and_then(|claim| claim.get("session"))
                    .and_then(serde_json::Value::as_str)
                    == Some(catalog.session()))
                .then_some(node.as_str())
            })
        })
        .collect();
    let known_facts: BTreeSet<&str> = analysis.facts.iter().map(|fact| fact.id.as_str()).collect();
    let jobs_are_complete = analysis.jobs.len() == expected_jobs.len()
        && analysis
            .jobs
            .iter()
            .zip(expected_jobs)
            .all(|(job, expected)| {
                job.job == expected
                    && job.scope == "at_export"
                    && canonical_prefixed(&job.result_id, "mind_", 20)
                    && job.rejected_rows <= 16
                    && canonical_digest(&job.prompt_digest)
                    && canonical_digest(&job.schema_digest)
                    && canonical_digest(&job.draft_digest)
                    && result_links_are_valid(&job.result, &known_facts, &known_nodes)
            });
    analysis.contract_version == CONTRACT_VERSION
        && analysis.prompt_version == PROMPT_VERSION
        && analysis.schema_version == SCHEMA_VERSION
        && analysis.tier == "language"
        && analysis.mode == "shadow"
        && !analysis.truth_bearing
        && analysis.model == MODEL
        && analysis.model_digest == MODEL_DIGEST
        && analysis.project == catalog.project()
        && analysis.session == catalog.session()
        && analysis.generated_at == catalog.generated_at()
        && analysis.source_digest == catalog.source_digest()
        && analysis.provider == "ollama"
        && analysis.coverage == *catalog.coverage()
        && analysis.facts == catalog.facts()
        && jobs_are_complete
        && analysis_digest_is_valid(analysis)
}

fn analysis_digest_is_valid(analysis: &GroundedAnalysis) -> bool {
    if !canonical_digest(&analysis.output_digest) {
        return false;
    }
    grounded_output_digest(analysis).is_ok_and(|digest| digest == analysis.output_digest)
}

fn result_links_are_valid(
    result: &GroundedJobResult,
    known_facts: &BTreeSet<&str>,
    known_nodes: &BTreeSet<&str>,
) -> bool {
    match result {
        GroundedJobResult::Findings { findings } => {
            !findings.is_empty()
                && findings.iter().all(|row| {
                    row_links_are_valid(
                        &row.id,
                        &row.fact_ids,
                        &row.receipts,
                        known_facts,
                        known_nodes,
                    )
                })
        }
        GroundedJobResult::Proposals {
            decisions,
            next_steps,
        } => {
            (!decisions.is_empty() || !next_steps.is_empty())
                && decisions.iter().all(|row| {
                    row_links_are_valid(
                        &row.id,
                        &row.fact_ids,
                        &row.receipts,
                        known_facts,
                        known_nodes,
                    )
                })
                && next_steps.iter().all(|row| {
                    row_links_are_valid(
                        &row.id,
                        &row.fact_ids,
                        &row.receipts,
                        known_facts,
                        known_nodes,
                    )
                })
        }
        GroundedJobResult::Abstained { reason } => {
            !reason.is_empty() && reason.len() <= 128 && reason.is_ascii()
        }
    }
}

fn row_links_are_valid(
    id: &str,
    fact_ids: &[String],
    receipts: &[ReceiptHandle],
    known_facts: &BTreeSet<&str>,
    known_nodes: &BTreeSet<&str>,
) -> bool {
    !fact_ids.is_empty()
        && canonical_prefixed(id, "row_", 20)
        && fact_ids
            .iter()
            .all(|fact| known_facts.contains(fact.as_str()))
        && receipts.iter().all(|receipt| {
            canonical_prefixed(&receipt.handle, "receipt_", 16)
                && known_nodes.contains(receipt.node.as_str())
        })
}

fn canonical_prefixed(value: &str, prefix: &str, hex_length: usize) -> bool {
    let Some(suffix) = value.strip_prefix(prefix) else {
        return false;
    };
    suffix.len() == hex_length
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn semantic_rejection(stream: &mut TcpStream) -> Result<()> {
    respond(
        stream,
        422,
        "application/json; charset=utf-8",
        br#"{"error":"invalid_interpretation_target"}"#,
    )
}

fn internal_mind_error(stream: &mut TcpStream) -> Result<()> {
    respond(
        stream,
        500,
        "application/json; charset=utf-8",
        br#"{"error":"internal_error"}"#,
    )
}

fn mind_response(stream: &mut TcpStream, cached: bool, analysis: &GroundedAnalysis) -> Result<()> {
    let body = serde_json::to_vec(&serde_json::json!({
        "cached": cached,
        "analysis": analysis,
    }))?;
    respond(stream, 200, "application/json; charset=utf-8", &body)
}

/// Start a refresh and answer immediately.
///
/// It does NOT hold the connection open for the duration. The measured walk is
/// 12m26s over 90 projects (A143) and no browser will wait that long on one
/// request; a fetch that dies at a proxy timeout would leave the user staring at
/// a spinner while the work actually succeeded. So: a worker thread does the
/// walk, this returns 202, and the app polls `/api/state`.
///
/// The thread is not a daemon. It is owned by this process, it holds no port of
/// its own, and Ctrl+C takes it with everything else.
fn refresh(state: &Arc<State>, stream: &mut TcpStream) -> Result<()> {
    if state.busy.load(Ordering::SeqCst) {
        let body = br#"{"ok":false,"reason":"already_refreshing"}"#;
        return respond(stream, 409, "application/json; charset=utf-8", body);
    }
    let worker = Arc::clone(state);
    std::thread::spawn(move || {
        let Ok(_guard) = worker.refreshing.try_lock() else {
            return;
        };
        worker.busy.store(true, Ordering::SeqCst);
        say!();
        say!("Refreshing (you clicked Refresh in the app).");
        let outcome = run_export(&worker.db, &worker.dir, worker.diagnostics, true);
        let result = match outcome {
            Ok(report) => {
                say!(
                    "Read {} projects in {}.",
                    report.projects,
                    human_duration(report.elapsed.as_secs_f64())
                );
                serde_json::json!({
                    "ok": true,
                    "projects": report.projects,
                    "needing_attention": report.needing_attention,
                    "seconds": report.elapsed.as_secs_f64(),
                })
            }
            Err(err) => {
                // Rule 12: the browser is told the same thing the terminal is.
                say!("Refresh failed: {err:#}");
                serde_json::json!({ "ok": false, "detail": format!("{err:#}") })
            }
        };
        if let Ok(mut slot) = worker.last_result.lock() {
            *slot = Some(result);
        }
        worker.busy.store(false, Ordering::SeqCst);
    });
    respond(
        stream,
        202,
        "application/json; charset=utf-8",
        br#"{"ok":true,"started":true}"#,
    )
}

/// Sessions whose recording was written in the last five minutes, per vendor.
/// A count of files, not a claim about health — the provenance travels in the
/// payload so the UI can say exactly what was measured.
fn lanes_json() -> serde_json::Value {
    const WINDOW_SECS: u64 = 300;
    let fresh = |root: PathBuf, nested: bool| -> u64 {
        let now = std::time::SystemTime::now();
        let mut n = 0u64;
        let dirs: Vec<PathBuf> = if nested {
            std::fs::read_dir(&root)
                .map(|rd| {
                    rd.filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| p.is_dir())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![root]
        };
        for dir in dirs {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in rd.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "jsonl") {
                    continue;
                }
                let recent = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| now.duration_since(m).ok())
                    .is_some_and(|age| age.as_secs() <= WINDOW_SECS);
                if recent {
                    n += 1;
                }
            }
        }
        n
    };
    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
    serde_json::json!({
        "window_minutes": 5,
        "measured": "recordings written within the window, counted from file times",
        "claude_code": { "live": fresh(home.join(".claude/projects"), true),
                          "root": "~/.claude/projects" },
        "codex":       { "live": fresh(home.join(".codex/sessions"), true),
                          "root": "~/.codex/sessions" },
        "gemini":      { "roadmap": true },
    })
}

/// The component of the folder one file belongs to, as a stable key.
///
/// The screen does the naming: these keys are looked up in the copy file, so
/// the words a person reads ("preserved transcripts") live where every other
/// word a person reads lives, and this function never invents prose.
///
/// The classification is by the file's FIRST segment under the store folder,
/// which is what a person sees in Finder, and the ledger's own name is taken
/// from the file this server has open rather than assumed.
fn store_component(db_name: &str, first: &str) -> &'static str {
    let base = |n: &str| {
        n == db_name
            || n.strip_prefix(db_name)
                .is_some_and(|rest| rest == "-wal" || rest == "-shm")
    };
    if base(first) {
        return "records";
    }
    if first == "exec.db" || first == "exec.db-wal" || first == "exec.db-shm" {
        return "command_log";
    }
    match first {
        "mirrors" => "preserved_transcripts",
        "digest" => "digest",
        "export" => "export",
        _ => "other",
    }
}

/// `GET /api/storage` — what this product is keeping on this machine.
///
/// `L1.1` (`docs/loop/03-USER-LENS.md` rule 13) in one read field: the store
/// grows with every session and nothing in the product has ever said how big
/// it is, so a person finds out when the disk fills.
///
/// It measures THE FOLDER THE SCREEN NAMES, not one file inside it (T-2-10,
/// finding 1). The first version of this counted the ledger and its two SQLite
/// sidecars while the screen went on naming the whole folder, and on the store
/// the exit check measured that was 1.4 MB said against 3.0 MB kept — the gap
/// being mostly the preserved transcripts, which is the exact growth rule 13
/// was written to warn a person about. A number that is short by half, in a
/// sentence written to be believed, is worse than no number.
///
/// Every regular file under the folder is added up, read-only, with a
/// per-component breakdown for the technical layer. A symbolic link is skipped
/// rather than followed: a link's target is not kept in this folder, counting
/// it would be counting somebody else's bytes, and skipping links is also what
/// makes this walk terminate.
///
/// It reads no row and serves no recorded text, so there is nothing here for a
/// masker to do: a file's length is not a transcript, and a file's name never
/// leaves this function — only its component key does.
///
/// A file that is not there contributes nothing rather than failing the read:
/// a `-wal` exists only between checkpoints, and a store with no journal is
/// not a store that could not be measured. The same is true one level up — an
/// unreadable subfolder is skipped, and the total is still the sum of what
/// could be read.
///
/// **Two folders, when the product keeps two** (T-7-1 rehearsal). The command
/// log follows `THRULINE_HOME` and the ledger follows `HOME`; with the two set
/// apart, the folder above held everything except `exec.db`, and the sentence
/// said 1,094,002 bytes of 1,278,322 kept. `exec_home` is that second folder;
/// it is added — and returned, so the technical layer can name it — only when
/// it is a DIFFERENT folder: the same folder, one inside it, or one that
/// contains it (a `THRULINE_HOME` of `~` would otherwise count the whole home
/// directory) is not walked twice or walked at all.
///
/// **A folder that holds the records folder is not walked, and the command log
/// in it is still counted** (T-7-1 verifier). With `THRULINE_HOME` equal to
/// `HOME` the execution store is `~/exec.db`, outside the records folder, and
/// the rule above left it out: 1,094,002 bytes said against 1,278,322 kept.
/// That folder is somebody's home directory, so it is still never walked; the
/// execution store's own files in it — `exec.db` and its two SQLite sidecars,
/// the names [`store_component`] already calls `command_log` — are added one by
/// one, and the store's path is returned so the technical layer can name it.
fn store_size(db: &Path, exec_home: Option<&Path>) -> Kept {
    let root = db.parent().unwrap_or(db);
    let db_name = db
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let also = exec_home
        .filter(|other| a_separate_folder(root, other))
        .map(Path::to_path_buf);
    let holder = exec_home.filter(|other| holds_the_records_folder(root, other));

    let mut total = 0u64;
    let mut by_component: BTreeMap<&'static str, u64> = BTreeMap::new();
    // Each entry: the folder to read, and the component every file under it
    // belongs to (`None` at a root, where each entry decides for itself).
    let mut queue: Vec<(PathBuf, Option<&'static str>)> = vec![(root.to_path_buf(), None)];
    if let Some(other) = &also {
        queue.push((other.clone(), None));
    }
    while let Some((dir, component)) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            let here = component.unwrap_or_else(|| store_component(&db_name, &name));
            if kind.is_dir() {
                queue.push((entry.path(), Some(here)));
            } else if kind.is_file() {
                if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                    *by_component.entry(here).or_insert(0) += meta.len();
                }
            }
        }
    }

    // The command log in a folder that holds the records folder: its own
    // files, by name, and nothing else there. A link is skipped, as above.
    let mut also_command_log = None;
    if let Some(holder) = holder {
        for name in [EXEC_STORE, "exec.db-wal", "exec.db-shm"] {
            let Ok(meta) = std::fs::symlink_metadata(holder.join(name)) else {
                continue;
            };
            if meta.is_file() {
                total += meta.len();
                *by_component.entry("command_log").or_insert(0) += meta.len();
                // Named whenever any of its files was counted, so a number
                // that holds bytes from outside the records folder always
                // says where they are.
                also_command_log = Some(holder.join(EXEC_STORE));
            }
        }
    }

    // Largest first: the breakdown is read to find out what is growing.
    let mut parts: Vec<(&'static str, u64)> = by_component.into_iter().collect();
    parts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let parts = parts
        .into_iter()
        .map(|(key, bytes)| serde_json::json!({ "key": key, "bytes": bytes }))
        .collect();
    Kept {
        bytes: total,
        parts,
        also,
        also_command_log,
    }
}

/// The execution store's file name under `THRULINE_HOME`
/// (`thruline_ledger::Ledger::default_path`: `$THRULINE_HOME/exec.db`).
const EXEC_STORE: &str = "exec.db";

/// What this product is keeping on this machine: THE `L1.1` figure, and the
/// only function any route that states it may call.
///
/// Two routes say this number — `/api/storage` (the Inbox) and
/// `/api/first-use` (the first screen) — and they used to reach
/// [`store_size`] separately. The first-use route measured while it held the
/// ledger open, so every read counted SQLite's shared-memory file for that
/// connection: 32,768 bytes that are gone the moment it closes, and 1,311,090
/// said on one screen against 1,278,322 on the other for the same store
/// (T-7-1 verifier, `HTT-L11-EMPTY-FIRST-SCREEN`).
///
/// **Call it before the calling request opens the ledger.** A route's own
/// connection is not something this product keeps; measured first, both routes
/// give the store as it stands between requests.
struct Kept {
    bytes: u64,
    parts: Vec<serde_json::Value>,
    /// `THRULINE_HOME`, when it is a folder apart and was counted whole.
    also: Option<PathBuf>,
    /// `THRULINE_HOME/exec.db`, when `THRULINE_HOME` holds the records folder
    /// and only the execution store's own files in it were counted.
    also_command_log: Option<PathBuf>,
}

fn kept(state: &State) -> Kept {
    store_size(&state.db, state.exec_home.as_deref())
}

/// Is `other` a folder of its own — neither `root`, nor inside it, nor holding
/// it? Compared after resolving links, so `/var` and `/private/var` are one
/// place. A folder that does not exist holds nothing to count.
fn a_separate_folder(root: &Path, other: &Path) -> bool {
    let (Ok(root), Ok(other)) = (root.canonicalize(), other.canonicalize()) else {
        return false;
    };
    !(other.starts_with(&root) || root.starts_with(&other))
}

/// Does `other` hold `root` — contain it, without being it? Compared after
/// resolving links, like [`a_separate_folder`]; a folder that does not exist
/// holds nothing.
fn holds_the_records_folder(root: &Path, other: &Path) -> bool {
    let (Ok(root), Ok(other)) = (root.canonicalize(), other.canonicalize()) else {
        return false;
    };
    root != other && root.starts_with(&other)
}

fn storage(state: &State, query: &str, stream: &mut TcpStream) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Kept {
        bytes,
        parts,
        also,
        also_command_log,
    } = kept(state);
    // Version 3: `also` names the execution store's folder when it is kept
    // apart from `path` and was counted in `bytes`; null otherwise.
    // Version 4: `also_command_log` names the execution store's file when the
    // folder it is kept in holds `path` (THRULINE_HOME = HOME), so that folder
    // was not walked and only the store's own files in it were counted; null
    // otherwise. The two are never both set.
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": 4,
        "bytes": bytes,
        "path": state.db.parent().unwrap_or(&state.db).display().to_string(),
        "also": also.map(|p| p.display().to_string()),
        "also_command_log": also_command_log.map(|p| p.display().to_string()),
        "measured": "every file in the folder this server keeps its records in, and in `also` when the execution store is kept apart from it, and the execution store's own files at `also_command_log` when the folder it is kept in holds this one, added up",
        "parts": parts,
    }))?;
    respond(stream, 200, "application/json; charset=utf-8", &body)
}

fn state_json(state: &State) -> serde_json::Value {
    let index = state.dir.join("index.json");
    serde_json::json!({
        "have_export": index.is_file(),
        "refreshing": state.busy.load(Ordering::SeqCst),
        "last_refresh": state.last_result.lock().ok().and_then(|s| s.clone()),
        "generated_at": std::fs::read_to_string(&index)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["generated_at"].as_str().map(str::to_string)),
    })
}

/// The JSON API is a PROJECTION of the CLI, not a second implementation of it.
///
/// `/api/explain`, `/api/claims` and `/api/sessions` re-enter this same binary
/// and serve the bytes `explain --json` / `claims --json` / `sessions --json`
/// write to stdout when no path is excluded. That is deliberate
/// and it is the whole design: a second representation of the same fact is a
/// second thing to keep in sync, and the only way to be sure two shapes never
/// diverge is for there to be one shape. Nothing is added to the body — not a
/// wrapper, not a timestamp, and not a `schema_version` the CLI does not itself
/// emit (see `docs/UI-SERVER-CONTRACT.md`, "Versioning").
///
/// The two session projections apply a person's path exclusions to their
/// served JSON after the CLI has derived its answer. `/api/sessions` has no
/// transcript rows to filter and remains a byte-for-byte projection.
///
/// This is the session-selecting half, kept separate from [`spawn_projection`]
/// because a session list has no session to select: `sessions` takes neither
/// `--session` nor `--last`, and giving it one would be a shell-quoting bug
/// waiting to be found by whoever added the third verb.
/// The human words the Change routes serve beside the record.
///
/// The record's keys are the technical layer; these are the labels a person
/// reads, and they come from `thruline_core::voice` rather than from a string
/// literal here, so there is one place a wording is changed and one place the
/// vocabulary check looks.
fn change_labels_json() -> serde_json::Value {
    let mut labels = serde_json::Map::new();
    for term in thruline_core::voice::CHANGE_LABELS {
        labels.insert(
            term.slug.to_string(),
            serde_json::json!({"word": term.word, "sentence": term.sentence}),
        );
    }
    serde_json::Value::Object(labels)
}

/// Open the ledger for a read, or answer with the reason it could not be
/// opened. A pending schema upgrade is reported as itself — the route never
/// serves an empty list that would read as "you have no Changes".
fn open_ledger_for_read(
    state: &State,
    stream: &mut TcpStream,
) -> Result<Option<thruline_core::ledger::Ledger>> {
    match thruline_core::ledger::Ledger::open(&state.db) {
        Ok(ledger) => Ok(Some(ledger)),
        Err(thruline_core::error::CoreError::StoreHeld { .. }) => {
            store_held(state, stream)?;
            Ok(None)
        }
        Err(thruline_core::error::CoreError::UpgradePending { estimate, .. }) => {
            respond(
                stream,
                503,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({
                    "error": "upgrade_pending",
                    "message": format!("schema upgrade pending — run `thruline upgrade` ({estimate})"),
                }))?
                .as_slice(),
            )?;
            Ok(None)
        }
        Err(other) => Err(anyhow::Error::new(other).context("opening the ledger to read Changes")),
    }
}

/// All store-opening routes use one fixed, path-free refusal. The count is
/// kept in memory while HQ runs and saturates rather than wrapping.
fn store_held(state: &State, stream: &mut TcpStream) -> Result<()> {
    note_store_held(&state.store_held_refusals);
    respond(
        stream,
        503,
        "application/json; charset=utf-8",
        br#"{"error":"store_held","message":"ThruLine is removing a project's records; try again when it finishes"}"#,
    )
}

fn note_store_held(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
        Some(count.saturating_add(1))
    });
}

fn error_is_store_held(problem: &anyhow::Error) -> bool {
    problem.chain().any(|cause| {
        cause.downcast_ref::<ChildStoreHeld>().is_some()
            || matches!(
                cause.downcast_ref::<thruline_core::error::CoreError>(),
                Some(thruline_core::error::CoreError::StoreHeld { .. })
            )
    })
}

/// Exit 3 is also an ordinary policy verdict. The fixed, path-free stderr
/// distinguishes a held store from that valid verdict without probing the
/// store again after the child exits.
fn child_reported_store_held(output: &std::process::Output) -> bool {
    output.status.code() == Some(3)
        && output.stderr
            == b"ThruLine is removing a project's records; try again when it finishes\n"
}

/// `?project=<id>`, or nothing. Any other parameter is refused rather than
/// ignored, for the reason `/api/sessions` gives: a parameter that did nothing
/// is a caller believing it asked something.
fn project_param(query: &str) -> std::result::Result<Option<String>, &'static str> {
    if query.is_empty() {
        return Ok(None);
    }
    let mut project = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("project", value)) if !value.is_empty() && project.is_none() => {
                if !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                {
                    return Err("unknown_query_parameter");
                }
                project = Some(value.to_string());
            }
            _ => return Err("unknown_query_parameter"),
        }
    }
    Ok(project)
}

fn changes_index(state: &State, query: &str, stream: &mut TcpStream) -> Result<()> {
    let project = match project_param(query) {
        Ok(project) => project,
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
    // `read_changes` masks every free-text field before it returns, so nothing
    // here can serve raw stored text by forgetting a step.
    let changes = thruline_core::change::read_changes(&ledger, project.as_deref())
        .context("reading Changes from the ledger")?;
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
        "labels": change_labels_json(),
        "changes": changes,
    }))?;
    let roots = crate::exclusion_serve::project_roots(&ledger)?;
    let mut policies = BTreeMap::new();
    let mut presented: serde_json::Value = serde_json::from_slice(&body)?;
    let mut left_out = 0_u64;
    for change in presented["changes"]
        .as_array_mut()
        .context("Changes index has no changes")?
    {
        let project_id = change["project_id"]
            .as_str()
            .context("Change has no project")?;
        if !policies.contains_key(project_id) {
            let root = roots
                .get(project_id)
                .context("Change project has no root")?;
            policies.insert(
                project_id.to_owned(),
                crate::exclusion_serve::policy(&ledger, root)?,
            );
        }
        left_out += crate::exclusion_serve::checked_apply(change, &policies[project_id])?;
    }
    if left_out == 0 {
        respond(stream, 200, "application/json; charset=utf-8", &body)
    } else {
        presented[LEFT_OUT_FIELD] = serde_json::Value::from(left_out);
        respond(
            stream,
            200,
            "application/json; charset=utf-8",
            &serde_json::to_vec(&presented)?,
        )
    }
}

fn change_exclusion_policy(ledger: &Ledger, project_id: &str) -> Result<ExclusionPolicy> {
    let root = crate::exclusion_serve::project_roots(ledger)?
        .remove(project_id)
        .with_context(|| format!("Change project {project_id} has no recorded root"))?;
    crate::exclusion_serve::policy(ledger, &root)
}

/// Resolve the Change's presentation policy before an action can append its
/// record. An unknown Change keeps its existing action-specific refusal path.
fn change_action_policy(ledger: &Ledger, change_id: &str) -> Result<Option<ExclusionPolicy>> {
    let project_id: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM changes WHERE id = ?1",
            [change_id],
            |row| row.get(0),
        )
        .optional()?;
    project_id
        .map(|id| change_exclusion_policy(ledger, &id))
        .transpose()
}

fn change_action_policy_unavailable(stream: &mut TcpStream) -> Result<()> {
    respond(
        stream,
        503,
        "application/json; charset=utf-8",
        br#"{"error":"exclusion_policy_unavailable"}"#,
    )
}

/// Filter the already masked success document at the socket boundary. The
/// recorded act is untouched, including a person's original words.
fn change_action_success(
    stream: &mut TcpStream,
    body: &[u8],
    policy: Option<&ExclusionPolicy>,
) -> Result<()> {
    let Some(policy) = policy else {
        return change_action_policy_unavailable(stream);
    };
    match present_change_action(body, policy) {
        Ok(served) => respond(stream, 200, "application/json; charset=utf-8", &served),
        Err(_) => change_action_policy_unavailable(stream),
    }
}

fn present_change_action<'a>(body: &'a [u8], policy: &ExclusionPolicy) -> Result<Cow<'a, [u8]>> {
    crate::exclusion_serve::bytes(body, policy)
}

#[cfg(test)]
mod change_action_exclusion_tests {
    use super::*;
    use thruline_core::capture_hold::ExclusionView;

    #[test]
    fn five_change_action_success_shapes_leave_excluded_paths_out() {
        let policy = ExclusionPolicy::from_exclusions(
            "/synthetic/project",
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
        .expect("synthetic policy");
        let cases = [
            (
                "cancel",
                serde_json::json!({"cancelled": true, "name": "check private/secret.txt", "execution": [{"command": "cat private/secret.txt", "output": "PRIVATE_PATHLESS_BYTES"}]}),
            ),
            (
                "finding",
                serde_json::json!({"acted": {"reason": "see private/secret.txt"}, "findings": {"findings": [{"text": "private/secret.txt"}]}}),
            ),
            (
                "decide",
                serde_json::json!({"acted": {"changed": ["private/secret.txt"]}, "decisions": [{"note": "review private/secret.txt"}]}),
            ),
            (
                "waive",
                serde_json::json!({"recorded": {"rationale": "waive private/secret.txt"}}),
            ),
            (
                "scope",
                serde_json::json!({"accepted": {"allowed_scope": ["private/secret.txt", "src/public.rs"], "criteria": [{"text": "review private/secret.txt"}]}}),
            ),
        ];
        for (route, shape) in cases {
            let raw = serde_json::to_vec(&shape).expect("synthetic response");
            let served = present_change_action(&raw, &policy).expect(route);
            let text = String::from_utf8(served.into_owned()).expect(route);
            assert!(!text.contains("private/secret.txt"), "{route}: {text}");
            assert!(
                text.contains("left out by your exclusion"),
                "{route}: {text}"
            );
            let body: serde_json::Value = serde_json::from_str(&text).expect(route);
            assert!(
                body[LEFT_OUT_FIELD].as_u64().unwrap_or(0) > 0,
                "{route}: {text}"
            );
            assert!(String::from_utf8(raw)
                .expect(route)
                .contains("private/secret.txt"));
        }
    }
}

fn present_change_read<'a>(
    ledger: &Ledger,
    change_id: &str,
    body: &'a [u8],
) -> Result<Cow<'a, [u8]>> {
    let project_id: String = ledger.conn().query_row(
        "SELECT project_id FROM changes WHERE id = ?1",
        [change_id],
        |row| row.get(0),
    )?;
    let policy = change_exclusion_policy(ledger, &project_id)?;
    crate::exclusion_serve::bytes(body, &policy)
}

fn changes_one(state: &State, id: &str, query: &str, stream: &mut TcpStream) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    // STEP-7-GIT-LINE: the one route that reads git for the Change it serves
    // (`code_moved`). It reads and writes nothing else; the index does not.
    let found = thruline_core::change::read_change_live(&ledger, id)
        .context("reading one Change from the ledger")?;
    match found {
        Some(change) => {
            let policy = change_exclusion_policy(&ledger, &change.project_id)?;
            // BESIDE the Change, never inside it: where each confirmed
            // conversation's own log holds its final claim, a file and a
            // line, or why there is none. Never the words. A read; it writes
            // nothing, and nothing in the engine reads it back.
            let agent_log = thruline_core::agent_log::for_change(&ledger, &change)
                .context("locating what the agent said in its own log")?;
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
                "labels": change_labels_json(),
                "change": change,
                "agent_log": agent_log,
            }))?;
            let served = crate::exclusion_serve::bytes(&body, &policy)?;
            respond(stream, 200, "application/json; charset=utf-8", &served)
        }
        None => respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        ),
    }
}

/// Every association sentence, both registers, keyed by slug — the same table
/// the CLI reads. A surface cannot invent a sentence; it can only fail to find
/// one, and then it says so.
fn association_lines_json() -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for line in thruline_core::voice::ASSOCIATION_LINES {
        out.insert(
            line.slug.to_string(),
            serde_json::json!({"plain": line.plain, "technical": line.technical}),
        );
    }
    serde_json::Value::Object(out)
}

/// `POST /api/changes/<id>/associations` — one person's decision about one
/// link.
///
/// It can write exactly one row in exactly one table. There is no field in the
/// body that reaches a scope, a criterion or a result, which is `B3.16` held
/// by the shape of the request rather than by a check inside it.
fn change_association(
    state: &State,
    change_id: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct Request {
        session: String,
        decision: String,
        actor: Option<String>,
        reason: Option<String>,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "malformed_request"}))?.as_slice(),
        );
    };
    let provenance = match request.decision.as_str() {
        "confirm" => thruline_core::change::AssociationProvenance::Confirmed,
        "reject" => thruline_core::change::AssociationProvenance::Rejected,
        _ => {
            return respond(
                stream,
                400,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({"error": "unknown_decision"}))?.as_slice(),
            )
        }
    };
    // The refusal, in the copy file's words, before the store is opened.
    let Some(actor) = request.actor.filter(|a| !a.trim().is_empty()) else {
        let sentence = thruline_core::voice::association_line("confirm_needs_actor");
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": "actor_required",
                "plain": sentence.map(|l| l.plain),
                "technical": sentence.map(|l| l.technical),
            }))?
            .as_slice(),
        );
    };
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let at = thruline_core::ingest::iso8601_utc_now();
    match thruline_core::change_link::decide(
        &ledger,
        change_id,
        &request.session,
        provenance,
        &actor,
        request.reason.as_deref().unwrap_or_default(),
        &at,
    ) {
        Ok(id) => {
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::change_link::LINK_VIEW_SCHEMA_VERSION,
                "association": id,
                "provenance": provenance.as_str(),
                "asserted_at": at,
                "labels": change_labels_json(),
            }))?;
            respond(stream, 200, "application/json; charset=utf-8", &body)
        }
        Err(problem) => respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": "refused",
                "detail": problem.to_string(),
            }))?
            .as_slice(),
        ),
    }
}

/// Every coverage sentence, both registers, keyed by slug — the same table the
/// CLI reads. A surface cannot invent a sentence; it can only fail to find one,
/// and then it says so.
fn coverage_lines_json() -> serde_json::Value {
    let mut out = serde_json::Map::new();
    // The link layer's six refusal sentences travel with these, because the
    // reason a person has nothing to link is a fact about the LINK and the
    // sentence for it already exists. Looking it up beats writing a seventh.
    // T-3-2's sentences travel with them for the same reason: whether one item
    // was examined on this version is a fact about THAT ITEM, and the sentence
    // for it already exists.
    for line in thruline_core::voice::ASSOCIATION_LINES
        .iter()
        .chain(thruline_core::voice::COVERAGE_LINES)
        .chain(thruline_core::voice::INPUTS_LINES)
    {
        out.insert(
            line.slug.to_string(),
            serde_json::json!({"plain": line.plain, "technical": line.technical}),
        );
    }
    serde_json::Value::Object(out)
}

/// `GET /api/changes/<id>/coverage` — what the record holds for each thing that
/// counts as done.
///
/// **It brings the derived rows up to date before it serves them, and that is a
/// write.** Said here rather than hidden: `change_evidence::derive_links` is
/// deterministic, idempotent and append-only, so running it is the same kind of
/// act as `thruline rebuild` — the rows it writes are a function of the record
/// and nothing else, a second run writes nothing, and no row is ever edited or
/// removed. The alternative was a screen that said "no test ran for this" about
/// a criterion the rule had simply never been asked about, which is the one
/// thing this card exists to stop.
///
/// Everything served has been through the masker: `change_evidence::coverage`
/// masks at its own boundary and states what it did.
fn change_coverage(
    state: &State,
    change_id: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let exists: bool = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM changes WHERE id=?1",
            [change_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    if !exists {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        );
    }
    // T-3-5 — readiness travels WITH the coverage it was computed from, so the
    // screen can never show one document's obligations beside another
    // document's answer. Everything it names is already masked: it is derived
    // from this view and from typed records, and it introduces no new text.
    let (derived, view, readiness) = served_readiness(&ledger, change_id)?;
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": thruline_core::change_evidence::COVERAGE_VIEW_SCHEMA_VERSION,
        "labels": change_labels_json(),
        "lines": coverage_lines_json(),
        "readiness_lines": readiness_lines_json(),
        "coverage": view,
        "review_findings": review_findings_json(&readiness),
        // AI-REVIEW-OUT-OF-LAUNCH: whether this build offers AI review at
        // all. The Change screen offers none unless this is `true`.
        "ai_review": crate::integration_cmd::AI_REVIEW,
        "readiness": readiness,
        "derived": derived,
    }))?;
    let served = present_change_read(&ledger, change_id, &body)?;
    respond(stream, 200, "application/json; charset=utf-8", &served)
}

/// The readiness answer `/coverage` and `/readiness` serve, and the coverage
/// it was computed from: the derived links brought up to date first
/// (deterministic, idempotent, append-only), then one pure function over the
/// view. One function, so the two routes — and `thruline change attempts`,
/// which only asks whether the answer is Ready (T-4-4-ATTEMPT-BUDGET) — cannot
/// disagree about it.
pub(crate) fn served_readiness(
    ledger: &thruline_core::ledger::Ledger,
    change_id: &str,
) -> Result<(
    thruline_core::change_evidence::DeriveReport,
    thruline_core::change_evidence::CoverageView,
    thruline_core::change_readiness::Readiness,
)> {
    let at = thruline_core::ingest::iso8601_utc_now();
    let derived = thruline_core::change_evidence::derive_links(ledger, change_id, &at)
        .context("bringing the criterion links up to date")?;
    let environment = thruline_core::change_propose::machine_environment(ledger, change_id)
        .context("reading what this machine can run")?;
    let view = thruline_core::change_evidence::coverage(ledger, change_id, &environment)
        .context("reading what the record holds for each accepted criterion")?;
    let readiness = thruline_core::change_readiness::readiness(ledger, change_id, &view)
        .context("computing whether this version is ready for approval")?;
    Ok((derived, view, readiness))
}

/// T-4-1a-FIX (R-2026-09-28-5 (a)) — where the Change's accepted policy
/// expects a review, whether review findings can be had on this machine, and
/// when not, the plain sentence and why. `null` when no review is expected or
/// one can be had. It records nothing and changes no obligation: the review
/// obligation stays exactly as unmet as readiness computed it.
fn review_findings_json(
    readiness: &thruline_core::change_readiness::Readiness,
) -> serde_json::Value {
    let expected = readiness
        .obligations
        .iter()
        .any(|o| o.kind == thruline_core::change_readiness::ObligationKind::Review);
    if expected {
        crate::integration_cmd::review::review_availability().to_json()
    } else {
        serde_json::Value::Null
    }
}

/// Every readiness sentence, both registers, keyed by slug (T-3-5). A surface
/// cannot invent one; it can only fail to find one, and then it says so.
fn readiness_lines_json() -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for line in thruline_core::voice::READINESS_LINES {
        out.insert(
            line.slug.to_string(),
            serde_json::json!({"plain": line.plain, "technical": line.technical}),
        );
    }
    serde_json::Value::Object(out)
}

/// `GET /api/changes/<id>/readiness` — whether this version is ready for
/// approval, and what that answer rests on (T-3-5, `B7.6`-`B7.12`).
///
/// It brings the derived links up to date first, for the same reason
/// `/coverage` does and with the same guarantee: `derive_links` is
/// deterministic, idempotent and append-only. Readiness is then one pure
/// function over the view, so this route and the `readiness` block inside the
/// coverage document cannot disagree.
///
/// **This route is still a read.** Setting an obligation aside — waiving it,
/// or saying it does not apply — is an appended record with an actor, an
/// authority and a rationale, and since T-5-3 it has two write addresses of
/// its own under `/api/changes/<id>/obligations/<obligation>/`. This sentence
/// used to say the route was not built; it is corrected here rather than left
/// standing.
fn change_readiness_route(
    state: &State,
    change_id: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let exists: bool = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM changes WHERE id=?1",
            [change_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    if !exists {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        );
    }
    let (_, _, readiness) = served_readiness(&ledger, change_id)?;
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": thruline_core::change_readiness::READINESS_SCHEMA_VERSION,
        "lines": readiness_lines_json(),
        "review_findings": review_findings_json(&readiness),
        // AI-REVIEW-OUT-OF-LAUNCH: whether this build offers AI review at
        // all. The Change screen offers none unless this is `true`.
        "ai_review": crate::integration_cmd::AI_REVIEW,
        "readiness": readiness,
    }))?;
    let served = present_change_read(&ledger, change_id, &body)?;
    respond(stream, 200, "application/json; charset=utf-8", &served)
}

/// `GET /api/changes/<id>/export` — whether each record the Change's
/// taken-away copy rests on can be read now, and if not, why (T-5-4, `B10.16`).
///
/// A read, and only a read: `change_export::records` opens files and ledger
/// rows and writes nothing. Every path it serves went through the masker, and
/// the treatment is served beside it.
fn change_export_records(
    state: &State,
    change_id: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let at = thruline_core::ingest::iso8601_utc_now();
    match thruline_core::change_export::records(&ledger, change_id, &at)
        .context("looking at the records this Change's copy rests on")?
    {
        Some(records) => {
            let change = thruline_core::change::read_change(&ledger, change_id)?
                .context("export records have no Change")?;
            let policy = change_exclusion_policy(&ledger, &change.project_id)?;
            let body = serde_json::to_vec(&records)?;
            let served = crate::exclusion_serve::bytes(&body, &policy)?;
            respond(stream, 200, "application/json; charset=utf-8", &served)
        }
        None => respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        ),
    }
}

/// `GET /api/changes/<id>/reviews/<review>/raw` and `.../parsed` (T-4-1,
/// `B9.6`, R08, R12). The technical drill-down under one review.
///
/// What is served is a labelled, masked inspection copy, never the stored
/// bytes: `change_review::inspect_raw`/`inspect_parsed` decode encoded text,
/// mask every string, and name the original's digest beside the copy's. A
/// review whose parts no longer match its sealed digest is refused with the
/// reason as a code — there is no fallback to anything.
fn change_review_artifact(
    state: &State,
    path: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    let refuse = |stream: &mut TcpStream, status: u16, code: &str| -> Result<()> {
        respond(
            stream,
            status,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": code}))?.as_slice(),
        )
    };
    if !query.is_empty() {
        return refuse(stream, 400, "unknown_query_parameter");
    }
    let rest = &path["/api/changes/".len()..];
    let (rest, artifact) = if let Some(r) = rest.strip_suffix("/raw") {
        (r, "raw")
    } else if let Some(r) = rest.strip_suffix("/parsed") {
        (r, "parsed")
    } else {
        return refuse(stream, 404, "no_such_review");
    };
    let Some((change_id, review_id)) = rest.split_once("/reviews/") else {
        return refuse(stream, 404, "no_such_review");
    };
    if change_id.is_empty() || review_id.is_empty() || review_id.contains('/') {
        return refuse(stream, 404, "no_such_review");
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let read = if artifact == "raw" {
        thruline_core::change_review::inspect_raw(&ledger, change_id, review_id)
    } else {
        thruline_core::change_review::inspect_parsed(&ledger, change_id, review_id)
    }
    .context("reading one review's stored parts")?;
    use thruline_core::change_review::{InspectRefusal, Integrity};
    match read {
        Ok(inspection) => {
            let body = serde_json::to_vec(&inspection)?;
            let served = present_change_read(&ledger, change_id, &body)?;
            respond(stream, 200, "application/json; charset=utf-8", &served)
        }
        Err(InspectRefusal::NotFound) => refuse(stream, 404, "no_such_review"),
        Err(InspectRefusal::Integrity(Integrity::Missing)) => {
            refuse(stream, 409, "review_integrity_missing")
        }
        Err(InspectRefusal::Integrity(_)) => refuse(stream, 409, "review_integrity_altered"),
        Err(InspectRefusal::NotText) => refuse(stream, 422, "review_not_text"),
    }
}

/// Every applicability sentence, both registers, keyed by slug.
fn applicability_lines_json() -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for line in thruline_core::voice::APPLICABILITY_LINES {
        out.insert(
            line.slug.to_string(),
            serde_json::json!({"plain": line.plain, "technical": line.technical}),
        );
    }
    serde_json::Value::Object(out)
}

/// The version of the document `GET /api/changes/<id>/applicability` serves.
///
/// 1 → 2 (T-F-1-F1): each item gained `checked_paths`. The field is
/// `#[serde(default)]` and no consumer branches on this number, so nothing
/// breaks either way — it is bumped because an additive served field bumps the
/// version here, the way `CHANGE_VIEW_SCHEMA_VERSION` went to 3 at T-S-4. A
/// document that gains a field without saying so has a version that no longer
/// describes it, and the next reader has no way to tell the two shapes apart.
const APPLICABILITY_VIEW_SCHEMA_VERSION: u32 = 2;

/// `GET /api/changes/<id>/applicability` — whether each piece of linked
/// evidence still applies once the tree it was checked on has moved (T-S-2,
/// sprint 2026-09-22, narrowing T-3-3 `B6.4`-`B6.7`, `B7.3`, `B7.5`).
///
/// A read, and only a read: this route derives nothing and writes nothing.
/// A criterion link with no row here has never been assessed — that is a
/// fact about the record, not "still applies" guessed in its absence, so the
/// served map only carries the links a record actually exists for.
///
/// Everything served has been through the masker:
/// `evidence_applicability::ApplicabilityRecord::masked` masks the reason
/// paths at this route's own boundary.
///
/// SCHEMA VERSION 2 adds one field to every item: `checked_paths`, what the
/// rule compared (T-F-1-F1). Nothing else changed, and nothing was removed.
fn applicability_of_change(
    state: &State,
    change_id: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let exists: bool = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM changes WHERE id=?1",
            [change_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    if !exists {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        );
    }
    let standing = thruline_core::evidence_applicability::standing_for_change(&ledger, change_id)
        .context("reading which linked evidence still applies")?;
    let mut record = thruline_core::redact::MaskRecord::default();
    let items: Vec<serde_json::Value> = standing
        .values()
        .map(|item| {
            serde_json::to_value(item.masked(&mut record)).unwrap_or(serde_json::Value::Null)
        })
        .collect();
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": APPLICABILITY_VIEW_SCHEMA_VERSION,
        "labels": change_labels_json(),
        "lines": applicability_lines_json(),
        "items": items,
        "masking": record.to_treatment(),
    }))?;
    let served = present_change_read(&ledger, change_id, &body)?;
    respond(stream, 200, "application/json; charset=utf-8", &served)
}

/// `GET /api/changes/<id>/inputs` — what exactly will be examined (T-3-2,
/// `B4.6`, `B6.2`, `B6.3`).
///
/// A read, and only a read: unlike `/coverage` this route derives nothing and
/// writes nothing. What will be examined is written down by a person running
/// `thruline change prepare`, because observing a machine is an act and a
/// screen refresh is not.
///
/// Everything served has been through the masker: `examined_inputs::prepare_view`
/// masks at its own boundary and states what it did.
fn change_inputs(
    state: &State,
    change_id: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let exists: bool = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM changes WHERE id=?1",
            [change_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    if !exists {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        );
    }
    let view = thruline_core::examined_inputs::prepare_view(&ledger, change_id)
        .context("reading what will be examined for this Change")?;
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": thruline_core::examined_inputs::INPUTS_VIEW_SCHEMA_VERSION,
        "lines": inputs_lines_json(),
        "prepare": view,
    }))?;
    let served = present_change_read(&ledger, change_id, &body)?;
    respond(stream, 200, "application/json; charset=utf-8", &served)
}

/// Every examined-inputs sentence, in both registers, served beside the view
/// so the screen looks a slug up rather than building a sentence.
fn inputs_lines_json() -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for line in thruline_core::voice::INPUTS_LINES {
        map.insert(
            line.slug.to_string(),
            serde_json::json!({ "plain": line.plain, "technical": line.technical }),
        );
    }
    serde_json::Value::Object(map)
}

/// Whether a Change with this id exists, for the two T-4-2 routes.
fn change_exists(ledger: &thruline_core::ledger::Ledger, change_id: &str) -> bool {
    ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM changes WHERE id=?1",
            [change_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false)
}

/// `GET /api/changes/<id>/findings` — every finding of this Change's reviews
/// and where each one stands (T-4-2). Reads; writes nothing. Free text is
/// masked in `finding_standing::read` before it reaches this function.
fn change_findings(
    state: &State,
    change_id: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if !change_exists(&ledger, change_id) {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        );
    }
    let view = thruline_core::finding_standing::read(&ledger, change_id)
        .context("reading where each review finding stands")?;
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": thruline_core::finding_standing::FINDINGS_VIEW_SCHEMA_VERSION,
        "findings": view,
    }))?;
    let served = present_change_read(&ledger, change_id, &body)?;
    respond(stream, 200, "application/json; charset=utf-8", &served)
}

/// The standing route's body cap (ADV-review-input-2), sized from the one
/// limit both doors hold instead of the 512 bytes the other action routes
/// keep: three typed parts at `MAX_PART_CHARS` characters, every character
/// written the longest way JSON can write it (a `\uXXXX\uXXXX` surrogate pair,
/// twelve bytes), plus room for the fields nobody types. Anything a person can
/// save from a terminal fits; a body over even this could not hold three parts
/// within the limit, so it is refused with the same code, `too_long`.
const STANDING_BODY_MAX: usize = 3 * thruline_core::finding_standing::MAX_PART_CHARS * 12 + 4096;

/// `POST /api/changes/<id>/findings/<finding>/standing` — one person changes
/// where one thing a review found stands (T-4-2).
///
/// The body says where it stands now, who decided, under what authority, why,
/// and — for a fix only — which record shows it. Nothing in it reaches a
/// verdict, a link, a scope or a decision. `resolved_mark: true` records a
/// person marking the comment resolved, which reaches "waiting for a re-check"
/// and nothing further; the synthetic import is a terminal-only test door and
/// cannot be named from here.
///
/// A refusal is **409 with the refusal code** as `error` (and the parts that
/// were missing, or over the limit), so the screen renders the copy file's
/// sentence for it and never reads a refusal as saved.
fn change_finding_standing(
    state: &State,
    change_id: &str,
    finding: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    use thruline_core::finding_standing::{
        EvidenceKind, EvidenceRef, Outcome, Standing, StandingAct, Via,
    };
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Evidence {
        kind: String,
        id: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        to: String,
        #[serde(default)]
        resolved_mark: bool,
        #[serde(default)]
        actor: String,
        #[serde(default)]
        authority: String,
        #[serde(default)]
        reason: String,
        evidence: Option<Evidence>,
        /// The standing the screen was showing when the person clicked.
        seen: Option<String>,
    }
    let malformed = |stream: &mut TcpStream| {
        respond(
            stream,
            400,
            "application/json; charset=utf-8",
            br#"{"error":"malformed_request"}"#,
        )
    };
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return malformed(stream);
    };
    let Ok(to) = Standing::parse(request.to.trim()) else {
        return malformed(stream);
    };
    let seen = match request.seen.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s) => match Standing::parse(s) {
            Ok(s) => Some(s),
            Err(_) => return malformed(stream),
        },
    };
    let evidence = match request.evidence {
        None => None,
        Some(e) => match EvidenceKind::parse(e.kind.trim()) {
            Ok(kind) => Some(EvidenceRef { kind, id: e.id }),
            Err(_) => return malformed(stream),
        },
    };
    let Some(finding) = percent_decode(finding) else {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            br#"{"error":"no_such_finding"}"#,
        );
    };
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if !change_exists(&ledger, change_id) {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        );
    }
    let policy = match change_action_policy(&ledger, change_id) {
        Ok(policy) => policy,
        Err(_) => return change_action_policy_unavailable(stream),
    };
    let act = StandingAct {
        finding,
        to,
        via: if request.resolved_mark {
            Via::ResolvedMark
        } else {
            Via::Person
        },
        actor: request.actor,
        authority: request.authority,
        reason: request.reason,
        evidence,
        seen,
        at: thruline_core::ingest::iso8601_utc_now(),
    };
    let outcome = thruline_core::finding_standing::record(&ledger, change_id, &act)
        .context("recording where this finding stands")?;
    match outcome {
        Outcome::Refused(refusal) => respond(
            stream,
            409,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": refusal.code,
                "missing": refusal.missing,
                "too_long": refusal.too_long,
                "current": refusal.current,
            }))?
            .as_slice(),
        ),
        recorded => {
            let view = thruline_core::finding_standing::read(&ledger, change_id)
                .context("re-reading where each finding stands after the act")?;
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::finding_standing::FINDINGS_VIEW_SCHEMA_VERSION,
                "acted": recorded,
                "findings": view,
            }))?;
            change_action_success(stream, &body, policy.as_ref())
        }
    }
}

/// `POST /api/changes/<id>/links` — one person adds or removes one link.
///
/// It can write exactly one row in exactly one table, and the only two words it
/// will write in `basis` are the two a person is allowed to write. There is no
/// field in the body that reaches a scope, a criterion's text or a result, and
/// there is no field that deletes anything: removing a link appends a
/// tombstone.
fn change_links(state: &State, change_id: &str, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    use thruline_core::change_evidence::{EvidenceKind, LinkAct, LinkBasis};

    #[derive(serde::Deserialize)]
    struct Request {
        criterion: u32,
        kind: String,
        evidence: String,
        act: String,
        actor: Option<String>,
        reason: Option<String>,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "malformed_request"}))?.as_slice(),
        );
    };
    let basis = match request.act.as_str() {
        "link" => LinkBasis::Manual,
        "unlink" => LinkBasis::NotBearing,
        _ => {
            return respond(
                stream,
                400,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({"error": "unknown_act"}))?.as_slice(),
            )
        }
    };
    let Ok(kind) = EvidenceKind::parse(&request.kind) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_evidence_kind"}))?.as_slice(),
        );
    };
    // The refusal, in the copy file's words, before the store is opened.
    let Some(actor) = request.actor.filter(|a| !a.trim().is_empty()) else {
        let sentence = thruline_core::voice::coverage_line("link_needs_actor");
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": "actor_required",
                "plain": sentence.map(|l| l.plain),
                "technical": sentence.map(|l| l.technical),
            }))?
            .as_slice(),
        );
    };
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let at = thruline_core::ingest::iso8601_utc_now();
    let act = LinkAct {
        criterion_index: request.criterion,
        kind,
        evidence_id: request.evidence,
        basis,
        actor,
        reason: request.reason.unwrap_or_default(),
        at: at.clone(),
    };
    match thruline_core::change_evidence::act(&ledger, change_id, &act) {
        Ok(id) => {
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::change_evidence::COVERAGE_VIEW_SCHEMA_VERSION,
                "link": id,
                "basis": basis.as_str(),
                "asserted_at": at,
                "lines": coverage_lines_json(),
            }))?;
            respond(stream, 200, "application/json; charset=utf-8", &body)
        }
        Err(problem) => respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": "refused",
                "detail": problem.to_string(),
            }))?
            .as_slice(),
        ),
    }
}

/// Every proposal sentence, both registers, keyed by slug — the same table the
/// CLI reads. A surface cannot invent a sentence; it can only fail to find one,
/// and then it says so.
fn proposal_lines_json() -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for line in thruline_core::voice::PROPOSAL_LINES {
        out.insert(
            line.slug.to_string(),
            serde_json::json!({"plain": line.plain, "technical": line.technical}),
        );
    }
    serde_json::Value::Object(out)
}

/// `GET /api/changes/<id>/proposals` — what this Change could be judged
/// against, and what is already accepted.
///
/// Read-only by construction: the only function it calls that could write is not
/// called. Everything it serves has been through the masker, because
/// `change_propose::propose` masks at its own boundary and states what it did.
fn change_proposals(
    state: &State,
    change_id: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let environment = thruline_core::change_propose::machine_environment(&ledger, change_id)
        .context("reading what this machine can run")?;
    match thruline_core::change_propose::propose(&ledger, change_id, &environment) {
        Ok(view) => {
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::change_propose::PROPOSAL_VIEW_SCHEMA_VERSION,
                "labels": change_labels_json(),
                "lines": proposal_lines_json(),
                "proposals": view,
            }))?;
            let served = present_change_read(&ledger, change_id, &body)?;
            respond(stream, 200, "application/json; charset=utf-8", &served)
        }
        // The one failure `propose` has is "no such Change", and it is a 404
        // rather than a 500: a Change that does not exist is not a fault.
        Err(_) => respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        ),
    }
}

/// The longest name an approval or withdrawal accepts, in characters.
const DECISION_ACTOR_MAX_CHARS: usize = 200;

/// `POST /api/changes/<id>/decisions` and `.../decisions/withdraw` — one
/// person approves exactly one version, or takes that approval back (T-S-4).
///
/// There is no field in the body that reaches a branch, a remote, an agent or
/// a result. The request says who, when, and which version the screen was
/// showing; everything else the row carries is read off the record.
///
/// **C30 R-LEDGER.** Either request may carry a `note` (at most 2,000
/// characters). The act appends it in its own transaction, so the decision and
/// its note are recorded together or not at all; a note over the limit is
/// `409 too_long` naming `note`, and nothing is recorded. Neither request
/// accepts a field this function does not name.
///
/// A tree that moved while the screen was open answers **200 with
/// `outcome: "moved"`**, not a refusal status: the screen has to render what
/// changed, and an HTTP error would arrive at the generic refusal path where
/// the list of files has nowhere to go.
fn change_decide(
    state: &State,
    change_id: &str,
    body: &[u8],
    approving: bool,
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        actor: Option<String>,
        /// The version the screen was showing when the person clicked.
        #[serde(default)]
        seen_version: String,
        /// C30 R-LEDGER — words kept with this act, at most 2,000 characters.
        /// Appended by the act itself, in its transaction. Nothing but
        /// whitespace is no note.
        #[serde(default)]
        note: Option<String>,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "malformed_request"}))?.as_slice(),
        );
    };
    let Some(actor) = request.actor.filter(|a| !a.trim().is_empty()) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "actor_required"}))?.as_slice(),
        );
    };
    /* The body cap grew from 512 bytes to what a note needs, so the name — which
     * that cap used to bound — is bounded on its own. */
    if actor.chars().count() > DECISION_ACTOR_MAX_CHARS {
        return person_acts::too_long(stream, "actor", DECISION_ACTOR_MAX_CHARS);
    }
    if let Some(words) = request.note.as_deref() {
        if let Err(thruline_core::change_note::NoteProblem::TooLong { .. }) =
            thruline_core::change_note::check(words)
        {
            return person_acts::too_long(
                stream,
                "note",
                thruline_core::change_note::MAX_NOTE_CHARS,
            );
        }
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let policy = match change_action_policy(&ledger, change_id) {
        Ok(policy) => policy,
        Err(_) => return change_action_policy_unavailable(stream),
    };
    let decide = thruline_core::change_decision::Decide {
        actor,
        at: thruline_core::ingest::iso8601_utc_now(),
        seen_revision: request.seen_version,
        note: request.note,
    };
    /* T-DIST-4 — approving goes through the ONE gated door, which refuses
     * before the ledger is touched when this machine has no licence that
     * works. Withdrawing does not: taking back what you said is never paid. */
    let acted = if approving {
        match crate::licence_cmd::approve_version(&ledger, change_id, &decide) {
            Ok(crate::licence_cmd::Approval::Acted(outcome)) => Ok(outcome),
            Ok(crate::licence_cmd::Approval::Refused {
                reason,
                enable,
                status,
            }) => {
                return respond(
                    stream,
                    403,
                    "application/json; charset=utf-8",
                    serde_json::to_vec(&serde_json::json!({
                        "error": "licence_required",
                        "status": status,
                        "reason": reason,
                        "enable": enable,
                    }))?
                    .as_slice(),
                );
            }
            Err(problem) => Err(problem),
        }
    } else {
        thruline_core::change_decision::withdraw(&ledger, change_id, &decide)
    };
    match acted {
        Ok(outcome) => {
            let decisions = thruline_core::change_decision::read(&ledger, change_id)
                .context("re-reading the decisions after the act")?;
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
                "acted": outcome,
                "decisions": decisions,
            }))?;
            change_action_success(stream, &body, policy.as_ref())
        }
        Err(problem) => respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": "refused",
                "detail": problem.to_string(),
            }))?
            .as_slice(),
        ),
    }
}

/// The Change and the obligation out of one exception address, percent-decoded
/// exactly once. `None` when the path is not one of ours, or when either half
/// does not decode — an address that cannot be read is refused, never guessed.
fn obligation_address(path: &str, suffix: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/api/changes/")?;
    let rest = rest.strip_suffix(suffix)?;
    let (change, obligation) = rest.split_once("/obligations/")?;
    if change.is_empty() || obligation.is_empty() {
        return None;
    }
    Some((percent_decode(change)?, percent_decode(obligation)?))
}

/// `POST /api/changes/<id>/obligations/<obligation>/waive` and
/// `.../not-applicable` — one named person sets one obligation aside, with a
/// reason (T-5-3, `B7.18`-`B7.22`).
///
/// **The authority is not in the request.** It is read off the accepted check
/// profile by `change_readiness::waive`, which is the same function
/// `thruline change waive` calls, so an exception written from a terminal and
/// one written from a screen are the same record with the same refusals behind
/// them. A Change whose accepted profile configures no exception path is
/// refused here in the copy file's words, and nothing is appended.
///
/// Nothing this route writes is evidence: a waiver clears an obligation
/// and is absent from `rests_on` by construction, and the coverage counts are
/// computed from the evidence items, which it does not touch.
fn change_waive(
    state: &State,
    change_id: &str,
    obligation_id: &str,
    kind: thruline_core::change_readiness::WaiverKind,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct Request {
        actor: Option<String>,
        rationale: Option<String>,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "malformed_request"}))?.as_slice(),
        );
    };
    let Some(actor) = request.actor.filter(|a| !a.trim().is_empty()) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "actor_required"}))?.as_slice(),
        );
    };
    let Some(rationale) = request.rationale.filter(|r| !r.trim().is_empty()) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "rationale_required"}))?.as_slice(),
        );
    };
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let exists: bool = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM changes WHERE id=?1",
            [change_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    if !exists {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_change"}))?.as_slice(),
        );
    }
    let policy = match change_action_policy(&ledger, change_id) {
        Ok(policy) => policy,
        Err(_) => return change_action_policy_unavailable(stream),
    };
    let at = thruline_core::ingest::iso8601_utc_now();
    match thruline_core::change_readiness::waive(
        &ledger,
        change_id,
        obligation_id,
        kind,
        &actor,
        &rationale,
        &at,
    ) {
        Ok(record) => {
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::change_readiness::READINESS_SCHEMA_VERSION,
                "lines": readiness_lines_json(),
                "recorded": record,
            }))?;
            change_action_success(stream, &body, policy.as_ref())
        }
        Err(problem) => respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": "refused",
                "detail": problem.to_string(),
            }))?
            .as_slice(),
        ),
    }
}

/// How many lines one acceptance may carry as text (`lines`, C30).
const MAX_ACCEPT_LINES: usize = 40;

/// How many out-of-scope items one acceptance may add (`exclusions`, C30). The
/// same bound as the folders a Change is proposed to cover.
const MAX_OUT_OF_SCOPE: usize = thruline_core::change_propose::MAX_SCOPE_PATHS;

/// The scope/accept route's body cap (C30), sized from the parts a person can
/// type instead of the 512 bytes the other action routes keep: every line and
/// out-of-scope item, and as many rewrites as lines, at
/// `MAX_CRITERION_CHARS` characters, every character written the longest way
/// JSON can write it (twelve bytes); the folders, twice the proposed bound at
/// 512 bytes each; and room for the fields nobody types. A body over this
/// could not hold its parts within their limits, so it is refused whole (413).
const SCOPE_ACCEPT_BODY_MAX: usize = (2 * MAX_ACCEPT_LINES + MAX_OUT_OF_SCOPE)
    * thruline_core::change_propose::MAX_CRITERION_CHARS
    * 12
    + 2 * thruline_core::change_propose::MAX_SCOPE_PATHS * 512
    + 4096;

/// A typed part of a scope/accept body that cannot be written as sent:
/// `409` naming the part, its position and its limit, with a sentence for a
/// person. Checked before the store is opened, so nothing is written.
fn scope_part_refusal(
    stream: &mut TcpStream,
    error: &str,
    part: &str,
    index: Option<usize>,
    limit: usize,
    plain: String,
) -> Result<()> {
    respond(
        stream,
        409,
        "application/json; charset=utf-8",
        serde_json::to_vec(&serde_json::json!({
            "error": error,
            "part": part,
            "index": index,
            "limit": limit,
            "plain": plain,
        }))?
        .as_slice(),
    )
}

/// The `lines` and `exclusions` of one scope/accept body, checked: at most
/// `most` of them, each non-empty and at most `MAX_CRITERION_CHARS`
/// characters. `Ok(true)` when they pass; `Ok(false)` once a refusal has
/// been answered.
fn scope_parts_fit(
    stream: &mut TcpStream,
    part: &str,
    items: &[String],
    most: usize,
) -> Result<bool> {
    let chars = thruline_core::change_propose::MAX_CRITERION_CHARS;
    let (one, many) = if part == "lines" {
        ("line", "lines")
    } else {
        ("out-of-scope item", "out-of-scope items")
    };
    if items.len() > most {
        scope_part_refusal(
            stream,
            "too_many",
            part,
            None,
            most,
            format!(
                "One acceptance can hold at most {most} {many}, and this one has {}. Nothing was saved.",
                items.len()
            ),
        )?;
        return Ok(false);
    }
    for (index, item) in items.iter().enumerate() {
        if item.trim().is_empty() {
            scope_part_refusal(
                stream,
                "empty",
                part,
                Some(index),
                chars,
                format!("One {one} is empty. Nothing was saved. Write it, or leave it out."),
            )?;
            return Ok(false);
        }
        if item.trim().chars().count() > chars {
            scope_part_refusal(
                stream,
                "too_long",
                part,
                Some(index),
                chars,
                format!(
                    "One {one} is longer than {chars} characters. A paragraph is not one thing to check: split it, or shorten it. Nothing was saved."
                ),
            )?;
            return Ok(false);
        }
    }
    Ok(true)
}

/// `POST /api/changes/<id>/scope/accept` — one person writes down what counts
/// as done.
///
/// There is no field in the body that reaches a decision, an approval or a
/// result, which is `B5.7` held by the shape of the request rather than by a
/// check inside it.
///
/// C30 adds two optional fields, both written by this one act and by nothing
/// else: `lines` (lines accepted exactly as written — the person's own, or
/// lines of the standing version they keep) and `exclusions` (out of scope,
/// added to what the standing version already excludes). An unknown field is
/// refused (400), never dropped: a misspelt `exclusions` would otherwise lose
/// what a person marked out of scope without a word.
fn change_scope_accept(
    state: &State,
    change_id: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        #[serde(default)]
        all: bool,
        #[serde(default)]
        criteria: Vec<u32>,
        #[serde(default)]
        edits: std::collections::BTreeMap<String, String>,
        /// The person says they mean to drop or soften lines already accepted.
        #[serde(default)]
        replacing: bool,
        /// The folders this Change covers, when the person edited the proposed
        /// list before accepting it (T-3-2b). Absent means "the ones I was
        /// shown", which is the ordinary path; a list means the person's own,
        /// and the acceptance says whose it is.
        allowed_scope: Option<Vec<String>>,
        /// C30 — lines accepted exactly as written, after the chosen
        /// proposals.
        #[serde(default)]
        lines: Vec<String>,
        /// C30 — out of scope, added to what the standing version excludes.
        #[serde(default)]
        exclusions: Vec<String>,
        actor: Option<String>,
        rationale: Option<String>,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "malformed_request"}))?.as_slice(),
        );
    };
    if !scope_parts_fit(stream, "lines", &request.lines, MAX_ACCEPT_LINES)?
        || !scope_parts_fit(stream, "exclusions", &request.exclusions, MAX_OUT_OF_SCOPE)?
    {
        return Ok(());
    }
    let selection = match (
        request.all,
        request.criteria.is_empty(),
        request.lines.is_empty(),
    ) {
        (true, true, _) => thruline_core::change_propose::Selection::All,
        (false, false, _) => {
            thruline_core::change_propose::Selection::Numbers(request.criteria.clone())
        }
        // Only the person's own lines: no proposal is chosen, and that is a
        // choice, not an omission.
        (false, true, false) => thruline_core::change_propose::Selection::Numbers(Vec::new()),
        _ => {
            return respond(
                stream,
                400,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({"error": "say_all_or_which"}))?.as_slice(),
            )
        }
    };
    // The refusal, in the copy file's words, before the store is opened.
    let Some(actor) = request.actor.filter(|a| !a.trim().is_empty()) else {
        let sentence = thruline_core::voice::proposal_line("accept_needs_actor");
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": "actor_required",
                "plain": sentence.map(|l| l.plain),
                "technical": sentence.map(|l| l.technical),
            }))?
            .as_slice(),
        );
    };
    let mut edits = std::collections::BTreeMap::new();
    for (number, text) in &request.edits {
        let Ok(number) = number.parse::<u32>() else {
            return respond(
                stream,
                400,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({"error": "unknown_criterion_number"}))?
                    .as_slice(),
            );
        };
        edits.insert(number, text.clone());
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let policy = match change_action_policy(&ledger, change_id) {
        Ok(policy) => policy,
        Err(_) => return change_action_policy_unavailable(stream),
    };
    let environment = thruline_core::change_propose::machine_environment(&ledger, change_id)
        .context("reading what this machine can run")?;
    let at = thruline_core::ingest::iso8601_utc_now();
    match thruline_core::change_propose::accept_with(
        &ledger,
        change_id,
        &thruline_core::change_propose::Acceptance {
            selection,
            scope: match &request.allowed_scope {
                None => thruline_core::change_propose::ScopeChoice::AsProposed,
                Some(paths) => thruline_core::change_propose::ScopeChoice::Paths(paths.clone()),
            },
            replacing: request.replacing,
            edits,
            actor,
            rationale: request.rationale.unwrap_or_default(),
            accepted_at: at,
        },
        &thruline_core::change_propose::Additions {
            lines: request.lines.clone(),
            exclusions: request.exclusions.clone(),
        },
        &environment,
    ) {
        Ok(outcome) => {
            let body = serde_json::to_vec(&serde_json::json!({
                "schema_version": thruline_core::change_propose::PROPOSAL_VIEW_SCHEMA_VERSION,
                "labels": change_labels_json(),
                "lines": proposal_lines_json(),
                "accepted": outcome,
            }))?;
            change_action_success(stream, &body, policy.as_ref())
        }
        // Every refusal `accept` has is already a copy-file sentence, so the
        // route hands it over rather than writing a second wording for it.
        Err(problem) => respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({
                "error": "refused",
                "detail": problem.to_string(),
            }))?
            .as_slice(),
        ),
    }
}

/// `GET /api/sessions/<id>/changes` — the one field T-2-2 adds to a session's
/// read-only view. A session that belongs to nothing answers with an empty
/// list and the sentence that says so, never with a 404.
fn session_changes(
    state: &State,
    session_id: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let view = thruline_core::change_link::session_links(&ledger, session_id)
        .context("reading a session's Change links")?;
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": thruline_core::change_link::LINK_VIEW_SCHEMA_VERSION,
        "labels": change_labels_json(),
        "lines": association_lines_json(),
        "links": view,
    }))?;
    let project_id: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM sessions WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()?;
    let served = if let Some(project_id) = project_id {
        let policy = change_exclusion_policy(&ledger, &project_id)?;
        crate::exclusion_serve::bytes(&body, &policy)?
    } else {
        Cow::Borrowed(body.as_slice())
    };
    respond(stream, 200, "application/json; charset=utf-8", &served)
}

fn projection(
    state: &State,
    verb: &'static str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    let session = match session_param(query) {
        Ok(session) => session,
        Err(problem) => {
            return respond(
                stream,
                400,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({"error": problem}))?.as_slice(),
            )
        }
    };
    match &session {
        Some(id) => spawn_projection(state, verb, &["--session", id], stream),
        None => spawn_projection(state, verb, &["--last"], stream),
    }
}

/// Run `thruline <verb> --json <extra…>` and serve it under §14's exit rule.
///
/// §14's exit codes: 0 is ok and **3 is a policy answer, not a failure** — an
/// unsupported claim is exactly what a reader came to see. Both serve 200 and
/// the code is reported in a header, so a consumer can tell a policy answer
/// from a success without re-deriving the verdicts itself. Anything else is a
/// 502 naming the verb, the code, and the CLI's own stderr.
///
/// **`extra` is not in the header.** `X-ThruLine-Projection` names the verb and
/// `--json`, and only those: it is a pinned literal
/// (`crates/cli/tests/ui_server_contract.rs`), and a per-request session id in
/// it would make the value vary by request for the two verbs that already
/// shipped it.
fn spawn_projection(
    state: &State,
    verb: &'static str,
    args: &[&str],
    stream: &mut TcpStream,
) -> Result<()> {
    let mut command = std::process::Command::new(&state.cli_exe);
    // Re-enter the same explicit stores even when HOME is absent. Claims and
    // explain also resolve project policy from cwd, so inherit the configured
    // reviewed root rather than the directory that happened to launch HQ.
    command
        .arg("--db")
        .arg(&state.db)
        .arg(verb)
        .arg("--json")
        .args(args);
    if let Some(project) = &state.verification {
        command
            .arg("--exec-db")
            .arg(&project.db)
            .current_dir(&project.repo);
    }
    let output = command
        .stdin(std::process::Stdio::null())
        .output()
        .with_context(|| format!("running `thruline {verb} --json`"))?;

    if child_reported_store_held(&output) {
        return store_held(state, stream);
    }

    let code = output.status.code();
    let extra = [
        ("X-ThruLine-Projection", format!("{verb} --json")),
        (
            "X-ThruLine-Exit",
            code.map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
        ),
    ];
    match code {
        Some(0) | Some(3) => {
            let presented = if matches!(verb, "explain" | "claims") {
                present_session_projection(state, &output.stdout)
            } else if verb == "sessions" {
                Ledger::open(&state.db)
                    .map_err(anyhow::Error::from)
                    .and_then(|ledger| present_sessions_projection(&ledger, &output.stdout))
            } else {
                Ok(Cow::Borrowed(output.stdout.as_slice()))
            };
            match presented {
                Ok(body) => respond_with(
                    stream,
                    200,
                    "application/json; charset=utf-8",
                    &extra,
                    &body,
                ),
                Err(problem) if error_is_store_held(&problem) => store_held(state, stream),
                Err(_) => projection_policy_unavailable(stream, &extra),
            }
        }
        other => {
            if verb == "sessions" {
                // This list spans projects. One failed CLI stderr line could
                // name a path excluded by any of them, so serve a fixed error.
                let body = serde_json::to_vec(&serde_json::json!({
                    "error": "projection_failed",
                    "command": "thruline sessions --json",
                    "exit_code": other,
                    "stderr": "Session list could not be read."
                }))?;
                return respond_with(
                    stream,
                    502,
                    "application/json; charset=utf-8",
                    &extra,
                    &body,
                );
            }
            let raw_stderr = String::from_utf8_lossy(&output.stderr);
            let body = serde_json::to_vec(&serde_json::json!({
                "error": "projection_failed",
                "command": format!("thruline {verb} --json"),
                "exit_code": other,
                "stderr": bounded_stderr(&raw_stderr),
            }))?;
            let requested = (args.len() == 2 && args[0] == "--session").then(|| args[1]);
            match present_projection_failure(state, requested, &body, &raw_stderr) {
                Ok(presented) => respond_with(
                    stream,
                    502,
                    "application/json; charset=utf-8",
                    &extra,
                    &presented,
                ),
                Err(_) => projection_policy_unavailable(stream, &extra),
            }
        }
    }
}

/// Session lists can mix worktrees from several HQ projects. Apply each row's
/// enclosing project's choice, then keep the original CLI bytes on a miss.
fn present_sessions_projection<'a>(ledger: &Ledger, body: &'a [u8]) -> Result<Cow<'a, [u8]>> {
    let mut doc: serde_json::Value = serde_json::from_slice(body)?;
    let rows = doc["sessions"]
        .as_array_mut()
        .context("sessions projection has no rows")?;
    if rows.is_empty() {
        return Ok(Cow::Borrowed(body));
    }
    let projects = thruline_core::hq::hq_projects(ledger)?;
    let roots = crate::exclusion_serve::project_roots(ledger)?;
    let mut policies = std::collections::BTreeMap::new();
    let mut left_out = 0;
    for row in rows {
        let key = row["repo_key"]
            .as_str()
            .context("session row has no repository key")?;
        let observed = row["repo_root"]
            .as_str()
            .filter(|root| Path::new(root).is_absolute())
            .or_else(|| Path::new(key).is_absolute().then_some(key))
            .or_else(|| roots.get(key).map(String::as_str))
            .context("session row has no resolvable project root")?;
        let root = projects
            .iter()
            .filter(|project| Path::new(observed).starts_with(Path::new(&project.root)))
            .max_by_key(|project| project.root.len())
            .map(|project| project.root.as_str())
            .unwrap_or(observed);
        if !policies.contains_key(root) {
            policies.insert(
                root.to_owned(),
                crate::exclusion_serve::policy(ledger, root)?,
            );
        }
        let policy = policies
            .get(root)
            .context("session policy was not cached")?;
        let count = crate::exclusion_serve::checked_apply(row, policy)?;
        if count > 0 {
            row.as_object_mut()
                .context("session row lost its object")?
                .remove(LEFT_OUT_FIELD);
            left_out += count;
        }
    }
    if left_out == 0 {
        return Ok(Cow::Borrowed(body));
    }
    doc[LEFT_OUT_FIELD] = serde_json::Value::from(left_out);
    Ok(Cow::Owned(serde_json::to_vec(&doc)?))
}

#[cfg(test)]
mod sessions_exclusion_tests {
    use super::*;

    #[test]
    fn a_nested_worktree_root_is_hidden_in_the_mixed_session_list() {
        let temp = tempfile::tempdir().expect("temporary store");
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("synthetic ledger");
        let root = "/workspace/c46-sessions";
        ledger
            .upsert_project(
                &thruline_core::model::ProjectRef {
                    id: "member-session".into(),
                    root_path: root.into(),
                    source: thruline_core::model::Source::Codex,
                    display_name: None,
                },
                "2026-10-08T00:00:00Z",
            )
            .expect("synthetic project");
        let raw = br#"{"sessions":[{"id":"s-private","repo_key":"member-session","repo_root":"/workspace/c46-sessions/private","event_count":2},{"id":"s-public","repo_key":"member-session","repo_root":"/workspace/c46-sessions/public","event_count":1}],"total":2}"#;
        assert!(
            matches!(present_sessions_projection(&ledger, raw), Ok(Cow::Borrowed(bytes)) if bytes == raw)
        );
        let hold = thruline_core::capture_hold::Hold {
            project_id: "member-session",
            project_folder: root,
            actor: "synthetic person",
            recorded_at: "2026-10-08T00:00:01Z",
        };
        thruline_core::capture_hold::record_for_person(
            &ledger,
            thruline_core::capture_hold::Act::Excluded,
            &hold,
            Some("private"),
        )
        .expect("excluded path");
        let served = present_sessions_projection(&ledger, raw).expect("filtered sessions");
        let doc: serde_json::Value = serde_json::from_slice(&served).expect("session JSON");
        assert_eq!(doc["sessions"][0]["id"], "s-private");
        assert_eq!(doc["sessions"][0]["event_count"], 2);
        assert_eq!(
            doc["sessions"][1]["repo_root"],
            "/workspace/c46-sessions/public"
        );
        assert_eq!(doc[LEFT_OUT_FIELD], 1);
        assert!(!String::from_utf8_lossy(&served)
            .contains("\"repo_root\":\"/workspace/c46-sessions/private\""));
    }
}

/// The recorded session's project root selects the capture-hold policy. Use
/// the execution store's relationship when it has this session; otherwise a
/// captured session can resolve through the transcript ledger. The CLI's
/// current directory is not evidence of an older session's project.
fn present_session_projection<'a>(state: &State, body: &'a [u8]) -> Result<Cow<'a, [u8]>> {
    let doc: serde_json::Value = serde_json::from_slice(body).context("reading projected JSON")?;
    let session = doc["session"]
        .as_str()
        .context("projected JSON has no session id")?;
    // A stand-in can produce only an identity without a matching store. That
    // shape contains no presented text; any fuller answer needs a policy root.
    let identity_only = doc
        .as_object()
        .is_some_and(|fields| fields.len() == 1 && fields.contains_key("session"));
    let Some(policy) = projection_policy_for_session(state, session)? else {
        anyhow::ensure!(
            identity_only,
            "projected text has no stored session policy root"
        );
        return Ok(Cow::Borrowed(body));
    };
    crate::exclusion_serve::bytes(body, &policy)
}

fn projection_exec_db(state: &State) -> Result<PathBuf> {
    Ok(match &state.verification {
        Some(project) => project.db.clone(),
        None => match &state.exec_home {
            Some(home) => home.join("exec.db"),
            None => thruline_ledger::Ledger::default_path().context("locating execution store")?,
        },
    })
}

fn projection_policy_for_session(state: &State, session: &str) -> Result<Option<ExclusionPolicy>> {
    let ledger = Ledger::open(&state.db).context("reading projected session's capture policy")?;
    let exec_db = projection_exec_db(state)?;
    let repo: Option<(String, Option<String>)> = if exec_db.is_file() {
        let exec = thruline_ledger::Ledger::open_read_only(&exec_db)
            .context("reading projected session's execution store")?;
        exec.conn()
            .query_row(
                "SELECT r.key, r.root_path FROM sessions s JOIN repos r ON r.id = s.repo_id WHERE s.id = ?1",
                [session],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
    } else {
        None
    };
    if let Some((key, stored_root)) = repo {
        let observed_root =
            if let Some(root) = stored_root.filter(|root| Path::new(root).is_absolute()) {
                root
            } else if Path::new(&key).is_absolute() {
                key
            } else {
                crate::exclusion_serve::project_roots(&ledger)?
                    .remove(&key)
                    .with_context(|| {
                        format!("projected session {session} has no resolvable project root")
                    })?
            };
        // An execution session can come from a working root inside the HQ
        // project's repository. The person's hold is recorded at the HQ root.
        let root = thruline_core::hq::hq_projects(&ledger)?
            .into_iter()
            .filter(|project| Path::new(&observed_root).starts_with(Path::new(&project.root)))
            .max_by_key(|project| project.root.len())
            .map(|project| project.root)
            .unwrap_or(observed_root);
        return Ok(Some(crate::exclusion_serve::policy(&ledger, &root)?));
    }
    // Verification pins a repository even when its execution store has no
    // session row. A reused capture session id cannot choose another folder's
    // exclusions for that explicitly configured projection.
    if let Some(project) = &state.verification {
        return integration_policy_for_session(state, session, Some(&project.repo)).map(Some);
    }
    // A session may be in the capture ledger even though the execution store
    // has no row for it. Resolve that recorded project before serving text.
    let captured_project: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM sessions WHERE id = ?1",
            [session],
            |row| row.get(0),
        )
        .optional()?;
    let Some(project_id) = captured_project else {
        return Ok(None);
    };
    let root = crate::exclusion_serve::project_roots(&ledger)?
        .remove(&project_id)
        .with_context(|| format!("projected session {session} has no recorded project root"))?;
    Ok(Some(crate::exclusion_serve::policy(&ledger, &root)?))
}

/// A failed child can name a path in stderr even though it produced no
/// projection body. Use the requested session (or the CLI's last-session
/// selection) to apply its exclusion policy before serving that diagnostic.
fn present_projection_failure<'a>(
    state: &State,
    requested_session: Option<&str>,
    body: &'a [u8],
    raw_stderr: &str,
) -> Result<Cow<'a, [u8]>> {
    if raw_stderr.is_empty() {
        return Ok(Cow::Borrowed(body));
    }
    let session = match requested_session {
        Some(session) => Some(session.to_string()),
        None => {
            let db = projection_exec_db(state)?;
            if db.is_file() {
                let exec = thruline_ledger::Ledger::open_read_only(&db)?;
                crate::claims_cmd::last_session(&exec)?
            } else {
                None
            }
        }
    };
    let session = session.context("projection diagnostic has no session policy root")?;
    let policy = projection_policy_for_session(state, &session)?
        .context("projection diagnostic has no session policy root")?;
    if let Some(hidden) = hidden_diagnostic(raw_stderr, &policy)? {
        let mut doc: serde_json::Value = serde_json::from_slice(body)?;
        doc["stderr"] = serde_json::Value::String(hidden);
        doc[LEFT_OUT_FIELD] = serde_json::Value::from(1);
        Ok(Cow::Owned(serde_json::to_vec(&doc)?))
    } else {
        Ok(Cow::Borrowed(body))
    }
}

/// Match against the entire masked diagnostic before the existing bounded
/// presentation cuts it; a path straddling that cut must still be hidden.
fn hidden_diagnostic(raw: &str, policy: &ExclusionPolicy) -> Result<Option<String>> {
    let mut value = serde_json::Value::String(thruline_core::redact::mask_recorded(raw).0);
    if crate::exclusion_serve::checked_apply(&mut value, policy)? == 0 {
        return Ok(None);
    }
    Ok(value.as_str().map(str::to_string))
}

fn present_phase_projection<'a>(
    state: &State,
    body: &'a [u8],
    unparsed_raw: &[String],
) -> Result<Cow<'a, [u8]>> {
    if unparsed_raw.is_empty() {
        return present_session_projection(state, body);
    }
    let mut doc: serde_json::Value = serde_json::from_slice(body)?;
    let session = doc["session"]
        .as_str()
        .context("phase projection has no session")?;
    let policy = projection_policy_for_session(state, session)?
        .context("phase diagnostic has no session policy root")?;
    let mut hidden = Vec::new();
    for (index, raw) in unparsed_raw.iter().enumerate() {
        if let Some(marker) = hidden_diagnostic(raw, &policy)? {
            hidden.push((index, marker));
        }
    }
    if hidden.is_empty() {
        return crate::exclusion_serve::bytes(body, &policy);
    }
    let count = hidden.len() as u64;
    for (index, marker) in hidden {
        doc["unparsed"][index] = serde_json::Value::String(marker);
    }
    let other_count = crate::exclusion_serve::checked_apply(&mut doc, &policy)?;
    doc[LEFT_OUT_FIELD] = serde_json::Value::from(count + other_count);
    Ok(Cow::Owned(serde_json::to_vec(&doc)?))
}

fn projection_policy_unavailable(stream: &mut TcpStream, headers: &[(&str, String)]) -> Result<()> {
    respond_with(
        stream,
        503,
        "application/json; charset=utf-8",
        headers,
        br#"{"error":"exclusion_policy_unavailable"}"#,
    )
}

/// `GET /api/phases?session=<id>` — the verdict pipeline's phase log.
///
/// Runs `thruline claims --json --phases` and serves what the CLI wrote to
/// **stderr**: one JSON object per completed phase, with the pipeline's own
/// counts and measured times. Stdout — the `claims --json` body — is not
/// served here; only its `session` id is read, so a `--last` request can say
/// which session it reports on. `/api/claims` remains the byte-for-byte home
/// of that body when no path is excluded.
///
/// A stderr line that is not a phase object is returned under `unparsed`
/// (masked, then bounded, like a 502's stderr) rather than dropped: a warning
/// the CLI printed mid-pipeline is part of what happened.
fn phases(state: &State, query: &str, stream: &mut TcpStream) -> Result<()> {
    let session = match session_param(query) {
        Ok(session) => session,
        Err(problem) => {
            return respond(
                stream,
                400,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({"error": problem}))?.as_slice(),
            )
        }
    };
    let mut command = std::process::Command::new(&state.cli_exe);
    command
        .arg("--db")
        .arg(&state.db)
        .args(["claims", "--json", "--phases"]);
    match &session {
        Some(id) => command.args(["--session", id]),
        None => command.arg("--last"),
    };
    if let Some(project) = &state.verification {
        command
            .arg("--exec-db")
            .arg(&project.db)
            .current_dir(&project.repo);
    }
    let output = command
        .stdin(std::process::Stdio::null())
        .output()
        .context("running `thruline claims --json --phases`")?;

    if child_reported_store_held(&output) {
        return store_held(state, stream);
    }

    let code = output.status.code();
    let extra = [
        (
            "X-ThruLine-Projection",
            "claims --json --phases".to_string(),
        ),
        (
            "X-ThruLine-Exit",
            code.map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
        ),
    ];
    if !matches!(code, Some(0) | Some(3)) {
        let raw_stderr = String::from_utf8_lossy(&output.stderr);
        let body = serde_json::to_vec(&serde_json::json!({
            "error": "projection_failed",
            "command": "thruline claims --json --phases",
            "exit_code": code,
            "stderr": bounded_stderr(&raw_stderr),
        }))?;
        return match present_projection_failure(state, session.as_deref(), &body, &raw_stderr) {
            Ok(presented) => respond_with(
                stream,
                502,
                "application/json; charset=utf-8",
                &extra,
                &presented,
            ),
            Err(_) => projection_policy_unavailable(stream, &extra),
        };
    }

    let mut phases: Vec<serde_json::Value> = Vec::new();
    let mut unparsed: Vec<String> = Vec::new();
    let mut unparsed_raw: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) if value.get("phase").is_some() => phases.push(value),
            _ => {
                unparsed_raw.push(line.to_string());
                unparsed.push(bounded_stderr(line));
            }
        }
    }
    let session = match session {
        Some(id) => serde_json::Value::String(id),
        None => serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .ok()
            .and_then(|v| v.get("session").cloned())
            .unwrap_or(serde_json::Value::Null),
    };
    let mut body = serde_json::json!({
        "contract": "thruline.phases.v1",
        "session": session,
        "phases": phases,
    });
    if !unparsed.is_empty() {
        body["unparsed"] = serde_json::Value::Array(
            unparsed
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
    }
    let bytes = serde_json::to_vec(&body)?;
    match present_phase_projection(state, &bytes, &unparsed_raw) {
        Ok(presented) => respond_with(
            stream,
            200,
            "application/json; charset=utf-8",
            &extra,
            &presented,
        ),
        Err(problem) if error_is_store_held(&problem) => store_held(state, stream),
        Err(_) => projection_policy_unavailable(stream, &extra),
    }
}

/// A failed projection's stderr, **masked and then bounded** for the 502 body.
///
/// Both halves matter, and the order is one of them.
///
/// It is *rendered*: `docs/UI-SERVER-CONTRACT.md` §2.3 publishes this `stderr`
/// field to Track A's web UI and HTML report, so a person reads it. And it is a
/// subprocess's stderr — the one string on this path that nothing upstream has
/// redacted, because it never passed through the spool. That makes this mask
/// load-bearing rather than idempotent, the same standing as
/// `codex_rollout::bounded`.
///
/// Cutting first can slice a credential's body below its detector's threshold,
/// and the cut string is all any later masker would ever see: that is how a
/// token past character 105 of a 143-character failed command reached sealed
/// capsule bytes (A209-C3). Enumerated in `redact::TRUNCATION_SITES`, and the
/// ordering is asserted behaviourally by
/// `a_credential_straddling_the_stderr_cut_is_masked_before_it_is_cut`.
fn bounded_stderr(raw: &str) -> String {
    let masked = thruline_core::redact::mask_recorded(raw).0;
    thruline_core::redact::truncate_respecting_markers(&masked, 2000)
}

/// The `session` query parameter, or the refusal.
///
/// The value becomes an argv element, so the accepted set is narrow by
/// construction: a session id is an identifier, and anything that is not one is
/// refused before a process is spawned rather than escaped on the way past.
fn session_param(query: &str) -> std::result::Result<Option<String>, &'static str> {
    let mut found = None;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name != "session" {
            return Err("unknown_query_parameter");
        }
        if found.is_some() {
            return Err("session_given_twice");
        }
        let decoded = percent_decode(value).ok_or("undecodable_session")?;
        let ok = !decoded.is_empty()
            && decoded.len() <= 128
            && decoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && decoded != "."
            && !decoded.starts_with('-');
        if !ok {
            return Err("invalid_session");
        }
        found = Some(decoded);
    }
    Ok(found)
}

fn serve_fixture(state: &State, path: &str, stream: &mut TcpStream) -> Result<()> {
    let rest = &path["/fixtures/".len()..];
    let Some(rel) = safe_fixture_path(rest) else {
        return respond(stream, 404, "text/plain; charset=utf-8", b"not found");
    };
    let file = state.dir.join(rel);
    match std::fs::read(&file) {
        Ok(bytes) => {
            let presented = (|| -> Result<Cow<'_, [u8]>> {
                let ledger = Ledger::open(&state.db).context("opening the fixture's ledger")?;
                if rest == "index.json" {
                    present_index_fixture(&ledger, &bytes)
                } else {
                    present_project_fixture(&ledger, &bytes).map(|(body, _)| body)
                }
            })();
            match presented {
                Ok(body) => respond(stream, 200, "application/json; charset=utf-8", &body),
                Err(problem) if error_is_store_held(&problem) => store_held(state, stream),
                Err(_) => respond(
                    stream,
                    503,
                    "application/json; charset=utf-8",
                    br#"{"error":"exclusion_policy_unavailable"}"#,
                ),
            }
        }
        Err(_) => respond(
            stream,
            404,
            "application/json; charset=utf-8",
            br#"{"error":"no reading on disk yet"}"#,
        ),
    }
}

/// The UI stages the unfiltered derived reading on disk, then applies today's
/// policy to each read. The same stage can restore an entry immediately after
/// an include without rewriting the corpus or trusting a stale redacted file.
fn present_project_fixture<'a>(ledger: &Ledger, raw: &'a [u8]) -> Result<(Cow<'a, [u8]>, String)> {
    let doc: serde_json::Value =
        serde_json::from_slice(raw).context("reading the project fixture")?;
    let id = doc["project"]["id"]
        .as_str()
        .context("project fixture has no ledger project id")?;
    let root = crate::exclusion_serve::project_roots(ledger)?
        .remove(id)
        .with_context(|| format!("project fixture {id} has no recorded root"))?;
    let policy = crate::exclusion_serve::policy(ledger, &root)?;
    let fingerprint = policy.fingerprint().to_string();
    let presented = crate::exclusion_serve::bytes(raw, &policy)?;
    Ok((presented, fingerprint))
}

fn present_index_fixture<'a>(ledger: &Ledger, raw: &'a [u8]) -> Result<Cow<'a, [u8]>> {
    let mut policies = BTreeMap::<String, ExclusionPolicy>::new();
    for (id, root) in crate::exclusion_serve::project_roots(ledger)? {
        policies.insert(
            thruline_core::export::project_fixture_id(&id),
            crate::exclusion_serve::policy(ledger, &root)?,
        );
    }
    let mut doc: serde_json::Value =
        serde_json::from_slice(raw).context("reading the index fixture")?;
    let rows = doc["projects"]
        .as_array_mut()
        .context("index fixture has no project rows")?;
    let mut left_out = 0_u64;
    for row in rows {
        let stem = row["fixture"]
            .as_str()
            .context("index row has no project fixture id")?;
        let policy = policies
            .get(stem)
            .with_context(|| format!("index fixture {stem} has no recorded project"))?;
        if !policy.is_empty() {
            left_out += crate::exclusion_serve::checked_apply(row, policy)?;
        }
    }
    if left_out == 0 {
        return Ok(Cow::Borrowed(raw));
    }
    doc[LEFT_OUT_FIELD] = serde_json::Value::from(left_out);
    Ok(Cow::Owned(serde_json::to_vec(&doc)?))
}

/// Only the two shapes the §9 contract defines, and nothing that could climb out
/// of the export directory.
fn safe_fixture_path(rest: &str) -> Option<PathBuf> {
    fn plain(name: &str) -> bool {
        !name.is_empty()
            && name.ends_with(".json")
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            && !name.contains("..")
    }
    if plain(rest) {
        return Some(PathBuf::from(rest));
    }
    let name = rest.strip_prefix("projects/")?;
    plain(name).then(|| Path::new("projects").join(name))
}

/// Percent-decoding, refusing anything that decodes to a NUL or is not valid
/// UTF-8. A path that cannot be decoded is not guessed at.
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = value.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    if out.contains(&0) {
        return None;
    }
    String::from_utf8(out).ok()
}

/// The relative path a request may reach inside a static root, or `None`.
///
/// Decoding happens FIRST and the segment check happens on the decoded form,
/// because `%2e%2e` and `..` are the same request and a check that only sees the
/// raw text catches one of them.
fn safe_static_path(path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path)?;
    let mut rel = PathBuf::new();
    for segment in decoded.trim_start_matches('/').split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\\') {
            return None;
        }
        rel.push(segment);
    }
    (!rel.as_os_str().is_empty()).then_some(rel)
}

fn serve_static(root: &Path, path: &str, stream: &mut TcpStream) -> Result<()> {
    let shell = |stream: &mut TcpStream| match std::fs::read(root.join("index.html")) {
        Ok(bytes) => respond(stream, 200, "text/html; charset=utf-8", &bytes),
        // The index was verified present at startup, so losing it mid-run is a
        // real fault and says so rather than answering with an empty body.
        Err(err) => respond(
            stream,
            500,
            "text/plain; charset=utf-8",
            format!("the static directory lost its index.html: {err}").as_bytes(),
        ),
    };
    if path == "/" || path.is_empty() {
        return shell(stream);
    }
    let Some(rel) = safe_static_path(path) else {
        return respond(
            stream,
            403,
            "text/plain; charset=utf-8",
            b"outside the UI directory",
        );
    };
    let file = root.join(&rel);
    // Canonicalize before reading: a symlink inside the directory is still a way
    // out of it, and the segment check above cannot see one.
    if let Ok(resolved) = file.canonicalize() {
        if !resolved.starts_with(root) {
            return respond(
                stream,
                403,
                "text/plain; charset=utf-8",
                b"outside the UI directory",
            );
        }
        // Case is enforced HERE, not by the filesystem. `canonicalize` returns
        // the name on disk, so on a case-insensitive volume (macOS, the machine
        // this was built on) `/Assets/App.js` resolves to `assets/app.js` and
        // would be served — while the same request 404s on a case-sensitive CI
        // runner. Comparing the resolved relative path against the requested one
        // makes the server answer identically on both, so a fixture that passes
        // here cannot fail there for a reason nothing reported.
        let same_case = resolved
            .strip_prefix(root)
            .is_ok_and(|on_disk| on_disk == rel);
        if resolved.is_file() && same_case {
            let name = rel.to_string_lossy().to_string();
            let bytes = std::fs::read(&resolved).with_context(|| format!("reading {name}"))?;
            return respond(stream, 200, mime_for(&name), &bytes);
        }
        if resolved.is_file() {
            return respond(stream, 404, "text/plain; charset=utf-8", b"not found");
        }
    }
    // Single-page app: an extensionless path is a client route and gets the
    // shell. A NAMED FILE that is absent is a 404, never the shell — answering a
    // missing bundle with HTML is exactly how a blank page happens.
    if !rel
        .file_name()
        .map(|n| n.to_string_lossy().contains('.'))
        .unwrap_or(true)
    {
        return shell(stream);
    }
    respond(stream, 404, "text/plain; charset=utf-8", b"not found")
}

/// Content types for a supplied directory. Kept beside `build.rs`'s table on
/// purpose: the embedded set is known at build time, this one is not.
fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn serve_asset(state: &State, path: &str, stream: &mut TcpStream) -> Result<()> {
    if let Some(root) = &state.static_root {
        return serve_static(root, path, stream);
    }
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    if let Some((_, mime, bytes)) = assets::UI_ASSETS.iter().find(|(p, _, _)| *p == rel) {
        return respond(stream, 200, mime, bytes);
    }
    // Single-page app: a path with no file extension is a route, not a missing
    // file, and it gets the shell.
    if !rel.rsplit('/').next().unwrap_or("").contains('.') {
        if let Some((_, mime, bytes)) = assets::UI_ASSETS
            .iter()
            .find(|(p, _, _)| *p == "index.html")
        {
            return respond(stream, 200, mime, bytes);
        }
    }
    respond(stream, 404, "text/plain; charset=utf-8", b"not found")
}

fn respond(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) -> Result<()> {
    respond_with(stream, status, mime, &[], body)
}

fn respond_with(
    stream: &mut TcpStream,
    status: u16,
    mime: &str,
    extra: &[(&str, String)],
    body: &[u8],
) -> Result<()> {
    if let Some(presented) = decisions_boundary::present(status, mime, body) {
        return match presented {
            Ok(bytes) => write_response(stream, status, mime, extra, &bytes),
            // The original body has not reached the socket. Write one fixed
            // refusal directly, avoiding a second pass through this hook.
            Err(_) => write_response(
                stream,
                503,
                "application/json; charset=utf-8",
                &[],
                br#"{"error":"exclusion_policy_unavailable"}"#,
            ),
        };
    }
    write_response(stream, status, mime, extra, body)
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    mime: &str,
    extra: &[(&str, String)],
    body: &[u8],
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Content",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        507 => "Insufficient Storage",
        _ => "OK",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {mime}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n",
        body.len()
    );
    for (name, value) in extra {
        // Header values are built here from literals and integers only. A value
        // carrying CR or LF would be a response-splitting bug, so it is refused
        // rather than sanitized.
        debug_assert!(!value.contains(['\r', '\n']));
        if !value.contains(['\r', '\n']) {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn port_of(origin: &str) -> &str {
    origin.rsplit(':').next().unwrap_or("")
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let launcher = ("open", vec![url]);
    #[cfg(target_os = "linux")]
    let launcher = ("xdg-open", vec![url]);
    #[cfg(target_os = "windows")]
    let launcher = ("cmd", vec!["/C", "start", "", url]);

    // `open` hands the URL to the desktop and exits; it is not a child we keep.
    // If it fails the URL is already printed above, so say so and carry on
    // rather than failing a command that otherwise worked.
    match std::process::Command::new(launcher.0)
        .args(&launcher.1)
        .status()
    {
        Ok(status) if status.success() => {}
        _ => say!("(Could not open a browser for you — open the address above.)"),
    }
}

fn export_age(index: &Path) -> Option<String> {
    let modified = std::fs::metadata(index).ok()?.modified().ok()?;
    let secs = std::time::SystemTime::now()
        .duration_since(modified)
        .ok()?
        .as_secs();
    Some(match secs {
        0..=90 => "just now".to_string(),
        91..=5400 => format!("{} minutes ago", secs / 60),
        5401..=172_800 => format!("{} hours ago", secs / 3600),
        _ => format!("{} days ago", secs / 86400),
    })
}

fn human_duration(seconds: f64) -> String {
    if seconds < 1.0 {
        return format!("{} ms", (seconds * 1000.0).round() as u64);
    }
    if seconds < 90.0 {
        return format!("{seconds:.1} seconds");
    }
    let minutes = seconds / 60.0;
    if minutes < 90.0 {
        return format!("{minutes:.1} minutes");
    }
    format!("{:.1} hours", minutes / 60.0)
}

/// Project ids are long paths. The progress line wants the tail.
fn short_project(id: &str) -> &str {
    id.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(id)
}

/// Read-only integration evidence, bounded by the same process/output budget as
/// the acceptance projection. The browser selects a known session, never a path
/// or executable. Run authorization stays at the explicit CLI plan boundary.
fn integration_projection(state: &State, query: &str, stream: &mut TcpStream) -> Result<()> {
    let session = match session_param(query) {
        Ok(Some(id)) => id,
        _ => {
            return respond(
                stream,
                400,
                "application/json",
                br#"{"error":"one_valid_session_required"}"#,
            )
        }
    };
    let Ok(_guard) = state.refreshing.try_lock() else {
        return respond(
            stream,
            503,
            "application/json",
            br#"{"error":"a_reading_is_in_progress"}"#,
        );
    };
    let mut cmd = std::process::Command::new(&state.cli_exe);
    cmd.arg("--db").arg(&state.db);
    let selected = review_projects::resolve(state, &session)?;
    if let Some((root, exec_db)) = &selected {
        cmd.args(["integration", "project", "--presentation", "--root"])
            .arg(root)
            .arg("--exec-db")
            .arg(exec_db);
    } else {
        cmd.args([
            "integration",
            "workspace",
            "--presentation",
            "--session",
            &session,
        ]);
    }
    match structured_projection(cmd) {
        Ok(bytes) => {
            let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
            value["session"] = serde_json::json!(session);
            let policy = match integration_policy_for_session(state, &session, selected.as_ref().map(|p| p.0.as_path())) {
                Ok(policy) => policy,
                Err(problem) if error_is_store_held(&problem) => return store_held(state, stream),
                Err(_) => return projection_policy_unavailable(stream, &[]),
            };
            if present_integration_evidence(&mut value, &policy).is_err() {
                return projection_policy_unavailable(stream, &[]);
            }
            respond(stream, 200, "application/json; charset=utf-8", &serde_json::to_vec(&value)?)
        },
        Err(problem) if error_is_store_held(&problem) => store_held(state, stream),
        Err(_) => respond(stream, 502, "application/json; charset=utf-8", br#"{"error":"Tool evidence could not be read. Inspect integration workspace in the terminal; no previous result is being shown."}"#),
    }
}

/// Resolve the capture policy from the integration project's recorded root.
/// Registered review projects supply that root directly; ordinary sessions
/// use the execution store's session-to-repository relationship.
fn integration_policy_for_session(
    state: &State,
    session: &str,
    selected_root: Option<&Path>,
) -> Result<ExclusionPolicy> {
    if let Some(observed) = selected_root {
        let ledger = Ledger::open(&state.db)?;
        let root = observed
            .to_str()
            .context("integration project root is not UTF-8")?;
        let hq_root = thruline_core::hq::hq_projects(&ledger)?
            .into_iter()
            .filter(|project| observed.starts_with(Path::new(&project.root)))
            .max_by_key(|project| project.root.len())
            .map(|project| project.root)
            .unwrap_or_else(|| root.to_owned());
        return crate::exclusion_serve::policy(&ledger, &hq_root);
    }
    projection_policy_for_session(state, session)?
        .context("integration session has no exclusion policy root")
}

fn integration_texts(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::String(text) => out.push(serde_json::Value::String(text.clone())),
        serde_json::Value::Array(items) => {
            for item in items {
                integration_texts(item, out);
            }
        }
        serde_json::Value::Object(fields) => {
            for item in fields.values() {
                integration_texts(item, out);
            }
        }
        _ => {}
    }
}

fn integration_identities(value: &serde_json::Value) -> Vec<(&'static str, serde_json::Value)> {
    [
        "binding_id",
        "artifact_digest",
        "acceptance_hash",
        "review_digest",
        "archive_sha256",
        "manifest_sha256",
        "export_id",
    ]
    .into_iter()
    .filter_map(|name| value.get(name).cloned().map(|value| (name, value)))
    .collect()
}

fn restore_integration_identities(
    value: &mut serde_json::Value,
    identities: Vec<(&'static str, serde_json::Value)>,
) {
    if let Some(fields) = value.as_object_mut() {
        for (name, original) in identities {
            fields.insert(name.to_owned(), original);
        }
    }
}

/// One saved integration run is one evidence entry: a path in its argv may
/// identify observations or diff lines that do not repeat that path. The
/// temporary summary lets the shared walker replace the whole run's display
/// text while leaving its binding identity, outcome and numeric facts intact.
fn present_integration_evidence(
    value: &mut serde_json::Value,
    policy: &ExclusionPolicy,
) -> Result<u64> {
    if policy.is_empty() {
        return Ok(0);
    }
    let mut identifiers = Vec::new();
    if let Some(runs) = value
        .pointer_mut("/results/runs")
        .and_then(serde_json::Value::as_array_mut)
    {
        for run in runs {
            identifiers.push(integration_identities(run));
            let mut summary = Vec::new();
            integration_texts(run, &mut summary);
            let evidence = run.take();
            *run = serde_json::json!({"summary": summary, "evidence": evidence});
        }
    }
    let left_out = crate::exclusion_serve::checked_apply(value, policy)?;
    if let Some(runs) = value
        .pointer_mut("/results/runs")
        .and_then(serde_json::Value::as_array_mut)
    {
        for (run, saved) in runs.iter_mut().zip(identifiers) {
            let evidence = run["evidence"].take();
            *run = evidence;
            restore_integration_identities(run, saved);
        }
    }
    Ok(left_out)
}

/// A POST result is one action's evidence. A path in its plan or file list
/// also hides pathless output from that same action.
fn present_integration_action(
    value: &mut serde_json::Value,
    policy: &ExclusionPolicy,
) -> Result<u64> {
    if policy.is_empty() {
        return Ok(0);
    }
    let identifiers = integration_identities(value);
    let mut summary = Vec::new();
    integration_texts(value, &mut summary);
    let evidence = value.take();
    let mut wrapped = serde_json::json!({"summary": summary, "evidence": evidence});
    let left_out = crate::exclusion_serve::checked_apply(&mut wrapped, policy)?;
    *value = wrapped["evidence"].take();
    restore_integration_identities(value, identifiers);
    if left_out > 0 {
        value[LEFT_OUT_FIELD] = serde_json::Value::from(left_out);
    }
    Ok(left_out)
}

#[cfg(test)]
mod tests {
    use std::net::Shutdown;
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Condvar;

    use tempfile::TempDir;
    use thruline_core::mind::MindJob;
    use thruline_core::mind_grounded::{
        analyze_with, FindingCategory, GroundedDraftClient, GroundedFinding, ModelIdentity,
    };

    use super::*;

    const PROJECT: &str = "0123456789abcdef";
    const SESSION: &str = "public-session-01";
    const ORIGIN: &str = "127.0.0.1:49991";

    /// A credential that STRADDLES the 2,000-character cut does not survive it.
    ///
    /// The ordering stated as a behaviour rather than as a comment. The token
    /// is built at runtime so this file carries no credential-shaped literal,
    /// and it is positioned to begin before the boundary and end after it — the
    /// geometry that let a token past character 105 of a 143-character command
    /// reach sealed capsule bytes when the cut came first (A209-C3).
    ///
    /// The assertion is on the token's PREFIX, not on a long slice of it. Cut
    /// first and the surviving head is a short fragment that no detector fires
    /// on, so it ships verbatim; asserting a 24-character slice was absent
    /// passed under the defect, because the defect never left 24 characters.
    #[test]
    fn a_credential_straddling_the_stderr_cut_is_masked_before_it_is_cut() {
        // Entropy matters: an all-one-character body does not resemble a
        // credential and the masker correctly declines it, so a fixture built
        // that way would prove nothing. This is the body shape
        // `a209_masking_corpus.rs` uses for its own GitHub canary.
        let token = format!("ghp_{}", "TLCANARYaaZZ11bbYY22ccXX33ddWW44e");
        // A separator before the token: `mask` walks whitespace-delimited
        // tokens, so a credential glued to preceding text is one long token
        // that starts with the wrong prefix. Real stderr has spaces in it.
        let straddling = format!("{} {token} trailing", "e".repeat(1_989));
        let out = bounded_stderr(&straddling);
        assert!(
            !out.contains("ghp_"),
            "a credential fragment survived the cut unmasked: {:?}",
            &out[out.len().saturating_sub(80)..]
        );
        assert!(out.chars().count() <= 2_000, "unbounded: {}", out.len());

        // ...and the mask really runs, rather than the cut merely discarding
        // the token — otherwise the assertion above would hold for the wrong
        // reason.
        assert!(bounded_stderr(&token).contains("[masked:"));
        // ...and the cut really runs, rather than the mask eating everything.
        assert_eq!(bounded_stderr(&"z".repeat(5_000)).chars().count(), 2_000);
        // ...and ordinary stderr is left alone.
        assert_eq!(bounded_stderr("no session found"), "no session found");
    }

    #[test]
    fn integration_route_is_read_only_scoped_and_fails_without_a_projection() {
        let dir = fixture_dir();
        let state = test_state(dir.path(), Arc::new(MockEngine::default()));
        for (method, query, expected) in [
            ("POST", "session=public-session-01", 405),
            ("GET", "", 400),
            ("GET", "session=public-session-01&command=delete", 400),
            ("GET", "session=public-session-01&session=other", 400),
            ("GET", "session=public-session-01", 502),
        ] {
            let req =
                format!("{method} /api/integrations?{query} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
            assert_eq!(status(&exchange(state.clone(), req.as_bytes())), expected);
        }
        let req = format!("GET /api/integrations?session=public-session-01 HTTP/1.1\r\nHost: {ORIGIN}\r\nOrigin: https://example.invalid\r\n\r\n");
        assert_eq!(status(&exchange(state, req.as_bytes())), 403);
    }

    #[test]
    fn integration_evidence_follows_exclude_and_include_at_serve_time() {
        let temp = fixture_dir();
        let exec = thruline_ledger::Ledger::open(&temp.path().join("exec.db"))
            .expect("synthetic execution store");
        let repo = exec
            .repo(
                "/workspace/thruline-synthetic",
                Some("/workspace/thruline-synthetic"),
            )
            .expect("synthetic repo");
        exec.upsert_session("s", &repo, "claude-code", None, false, None)
            .expect("synthetic session");
        drop(exec);

        let evidence = serde_json::json!({
            "schema": "thruline.integration-workspace.v1",
            "results": {"runs": [{
                "binding_id": "synthetic-binding",
                "record": {"plan": {"argv": ["cat", "src/module-00.rs"]},
                           "report": {"observations": [{"detail": "PRIVATE_OBSERVATION"}]}},
                "assessment": {"outcome": "observed"}
            }]}
        });
        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.exec_home = Some(temp.path().to_path_buf());
        state_mut.cli_exe = fake_cli(temp.path(), "integration-cli", &evidence.to_string(), 0);
        let request = format!("GET /api/integrations?session=s HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let baseline = exchange(state.clone(), request.as_bytes());
        assert_eq!(status(&baseline), 200, "{baseline}");
        let baseline_body = baseline.split("\r\n\r\n").nth(1).expect("body");
        assert!(baseline_body.contains("PRIVATE_OBSERVATION"));

        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("capture ledger");
        fixture_exclusion(&ledger, thruline_core::capture_hold::Act::Excluded);
        drop(ledger);
        let excluded = exchange(state.clone(), request.as_bytes());
        assert_eq!(status(&excluded), 200, "{excluded}");
        let hidden = response_json(&excluded);
        assert_eq!(hidden["left_out_by_exclusion"], 1);
        assert_eq!(
            hidden["results"]["runs"][0]["binding_id"],
            "synthetic-binding"
        );
        assert_eq!(
            hidden["results"]["runs"][0]["assessment"]["outcome"],
            "observed"
        );
        assert!(!excluded.contains("PRIVATE_OBSERVATION"), "{excluded}");
        assert!(
            excluded.contains("left out by your exclusion"),
            "{excluded}"
        );

        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("capture ledger");
        fixture_exclusion(&ledger, thruline_core::capture_hold::Act::Included);
        drop(ledger);
        let included = exchange(state, request.as_bytes());
        assert_eq!(status(&included), 200, "{included}");
        assert_eq!(
            included.split("\r\n\r\n").nth(1),
            Some(baseline_body),
            "including restores the original presentation bytes"
        );
    }

    #[test]
    fn an_integration_evidence_with_no_policy_root_fails_closed() {
        let temp = fixture_dir();
        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.exec_home = Some(temp.path().to_path_buf());
        state_mut.cli_exe = fake_cli(
            temp.path(),
            "unresolved-integration-cli",
            r#"{"results":{"runs":[{"record":{"plan":{"argv":["src/module-00.rs"]}}}]}}"#,
            0,
        );
        let response = exchange(
            state,
            format!("GET /api/integrations?session=s HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n")
                .as_bytes(),
        );
        assert_eq!(status(&response), 503, "{response}");
        assert!(!response.contains("src/module-00.rs"), "{response}");
    }

    #[test]
    fn one_integration_action_hides_pathless_output_from_an_excluded_file() {
        let temp = fixture_dir();
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("capture ledger");
        fixture_exclusion(&ledger, thruline_core::capture_hold::Act::Excluded);
        let policy = crate::exclusion_serve::policy(&ledger, "/workspace/thruline-synthetic")
            .expect("synthetic policy");
        let mut result = serde_json::json!({
            "binding_id": "synthetic-binding",
            "plan": {"argv": ["cat", "src/module-00.rs"]},
            "native_response": {"stdout": "PRIVATE_ACTION_OUTPUT"},
            "outcome": "observed"
        });
        assert_eq!(present_integration_action(&mut result, &policy).unwrap(), 1);
        assert_eq!(result["binding_id"], "synthetic-binding");
        assert_eq!(result["outcome"], "observed");
        assert_eq!(result["left_out_by_exclusion"], 1);
        assert!(!result.to_string().contains("PRIVATE_ACTION_OUTPUT"));
    }

    #[test]
    fn integration_actions_require_same_origin_explicit_json_and_reject_unknown_fields() {
        let temp = fixture_dir();
        let state = test_state(temp.path(), MockEngine::available());
        let body =
            br#"{"session":"public-session-01","action":"run","recipe":"ast-grep","digest":"x"}"#;
        assert_eq!(
            status(&exchange(
                state.clone(),
                &framed("POST", "/api/integrations/action", body, "")
            )),
            403
        );
        assert_eq!(
            status(&exchange(
                state.clone(),
                &framed("GET", "/api/integrations/action", b"", "")
            )),
            405
        );
        let accepted_headers = format!("Origin: http://{ORIGIN}\r\n");
        // Session IDs use the session contract, not 16-hex project IDs.
        assert_eq!(
            status(&exchange(
                state.clone(),
                &framed("POST", "/api/integrations/action", body, &accepted_headers)
            )),
            409
        );
        let unknown = br#"{"session":"public-session-01","action":"run","recipe":"ast-grep","argv":["/bin/sh"]}"#;
        let headers = format!("Origin: http://{ORIGIN}\r\n");
        assert_eq!(
            status(&exchange(
                state.clone(),
                &framed("POST", "/api/integrations/action", unknown, &headers)
            )),
            400
        );
        let duplicate = br#"{"session":"a","session":"b","action":"run","recipe":"ast-grep"}"#;
        assert_eq!(
            status(&exchange(
                state,
                &framed("POST", "/api/integrations/action", duplicate, &headers)
            )),
            400
        );
    }

    #[test]
    fn an_integration_action_refuses_before_starting_when_policy_cannot_be_read() {
        let temp = fixture_dir();
        let unusable_db = temp.path().join("unusable-ledger");
        std::fs::create_dir(&unusable_db).expect("unusable ledger path");
        let sentinel = temp.path().join("integration-started");
        let script = temp.path().join("integration-action-cli");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ntouch '{}'\nprintf '%s' '{{\"result\":\"started\"}}'\n",
                sentinel.display()
            ),
        )
        .expect("fake action CLI");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.db = unusable_db;
        state_mut.cli_exe = script;
        state_mut.verification = Some(LocalVerification {
            repo: temp.path().to_path_buf(),
            db: temp.path().join("exec.db"),
            job: Mutex::new(LocalJob::default()),
        });
        let body = br#"{"session":"local-verification","action":"run","recipe":"ast-grep","digest":"synthetic"}"#;
        let response = exchange(
            state,
            &framed("POST", "/api/integrations/action", body, &same_origin()),
        );
        assert_eq!(status(&response), 503, "{response}");
        assert!(!sentinel.exists(), "the action process must not start");
    }

    fn representative() -> &'static [u8] {
        include_bytes!("../../core/tests/fixtures/grounded-mind-public.json")
    }

    struct EmptyDraft;

    impl GroundedDraftClient for EmptyDraft {
        fn draft(
            &self,
            job: MindJob,
            _system: &str,
            _user: &str,
            _response_format: &serde_json::Value,
        ) -> std::result::Result<String, GroundedError> {
            Ok(match job {
                MindJob::SessionNarration => r#"{"findings":[]}"#.to_string(),
                MindJob::DecisionProposal => r#"{"decisions":[],"next_steps":[]}"#.to_string(),
                MindJob::CapsuleCompaction => {
                    return Err(GroundedError::Provider(
                        "unexpected compaction job".to_string(),
                    ))
                }
            })
        }
    }

    #[derive(Default)]
    struct BlockControl {
        state: Mutex<(bool, bool)>,
        changed: Condvar,
    }

    impl BlockControl {
        fn enter_and_wait(&self) {
            let mut state = self.state.lock().expect("block state");
            state.0 = true;
            self.changed.notify_all();
            while !state.1 {
                state = self.changed.wait(state).expect("block wait");
            }
        }

        fn wait_until_entered(&self) {
            let mut state = self.state.lock().expect("block state");
            while !state.0 {
                state = self.changed.wait(state).expect("entered wait");
            }
        }

        fn release(&self) {
            let mut state = self.state.lock().expect("block state");
            state.1 = true;
            self.changed.notify_all();
        }
    }

    #[derive(Default)]
    struct MockEngine {
        probes: AtomicUsize,
        analyses: AtomicUsize,
        available: AtomicBool,
        failures: AtomicUsize,
        partials: AtomicUsize,
        block: Mutex<Option<Arc<BlockControl>>>,
    }

    impl MockEngine {
        fn available() -> Arc<Self> {
            Arc::new(Self {
                available: AtomicBool::new(true),
                ..Self::default()
            })
        }

        fn take(counter: &AtomicUsize) -> bool {
            counter
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    if value > 0 {
                        Some(value - 1)
                    } else {
                        None
                    }
                })
                .is_ok()
        }
    }

    impl MindEngine for MockEngine {
        fn probe(&self) -> std::result::Result<ModelIdentity, GroundedError> {
            self.probes.fetch_add(1, Ordering::SeqCst);
            if self.available.load(Ordering::SeqCst) {
                Ok(ModelIdentity {
                    tag: MODEL.to_string(),
                    digest: MODEL_DIGEST.to_string(),
                })
            } else {
                Err(GroundedError::ModelUnavailable(
                    "PRIVATE_PROVIDER_FAILURE_DETAIL".to_string(),
                ))
            }
        }

        fn analyze(
            &self,
            catalog: &FactCatalog,
        ) -> std::result::Result<GroundedAnalysis, GroundedError> {
            self.analyses.fetch_add(1, Ordering::SeqCst);
            self.probes.fetch_add(1, Ordering::SeqCst);
            if let Some(block) = self.block.lock().expect("block slot").clone() {
                block.enter_and_wait();
            }
            if !self.available.load(Ordering::SeqCst) || Self::take(&self.failures) {
                return Err(GroundedError::Provider(
                    "PRIVATE_PROVIDER_FAILURE_DETAIL".to_string(),
                ));
            }
            let identity = ModelIdentity {
                tag: MODEL.to_string(),
                digest: MODEL_DIGEST.to_string(),
            };
            let mut analysis = analyze_with(catalog, &identity, &EmptyDraft)?;
            if Self::take(&self.partials) {
                analysis.jobs.pop();
            }
            Ok(analysis)
        }
    }

    fn fixture_dir() -> TempDir {
        let temp = tempfile::tempdir().expect("temp dir");
        let projects = temp.path().join("projects");
        std::fs::create_dir_all(&projects).expect("projects dir");
        std::fs::write(projects.join(format!("{PROJECT}.json")), representative())
            .expect("fixture");
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("fixture ledger");
        ledger
            .upsert_project(
                &thruline_core::model::ProjectRef {
                    id: PROJECT.into(),
                    root_path: "/workspace/thruline-synthetic".into(),
                    source: thruline_core::model::Source::Codex,
                    display_name: Some("Public Synthetic Project".into()),
                },
                "2026-08-25T00:00:00Z",
            )
            .expect("fixture project");
        temp
    }

    fn fixture_exclusion(ledger: &Ledger, act: thruline_core::capture_hold::Act) {
        let hold = thruline_core::capture_hold::Hold {
            project_id: PROJECT,
            project_folder: "/workspace/thruline-synthetic",
            actor: "synthetic person",
            recorded_at: match act {
                thruline_core::capture_hold::Act::Excluded => "2026-08-25T00:00:01Z",
                thruline_core::capture_hold::Act::Included => "2026-08-25T00:00:02Z",
                _ => unreachable!("this helper only records path choices"),
            },
        };
        thruline_core::capture_hold::record_for_person(
            ledger,
            act,
            &hold,
            Some("src/module-00.rs"),
        )
        .expect("record fixture exclusion");
    }

    fn test_state(dir: &Path, engine: Arc<MockEngine>) -> Arc<State> {
        Arc::new(State {
            observer: None,
            verification: None,
            shutting_down: Arc::new(AtomicBool::new(false)),
            acceptance: None,
            db: dir.join("ledger.db"),
            exec_home: None,
            dir: dir.to_path_buf(),
            diagnostics: false,
            static_root: None,
            cli_exe: PathBuf::from("/nonexistent/thruline"),
            refreshing: Mutex::new(()),
            busy: AtomicBool::new(false),
            store_held_refusals: AtomicU64::new(0),
            last_result: Mutex::new(None),
            origin: ORIGIN.to_string(),
            mind: MindRuntime {
                engine,
                in_flight: AtomicBool::new(false),
                cache: Mutex::new(MindCache::default()),
            },
            run_route: run_route::RunRoute::new(),
            read_cache: read_route::ReadCache::new(),
            hq: hq::HqState::default(),
        })
    }

    #[test]
    fn hq_read_refuses_a_held_scratch_store_then_reports_the_refusal_count() {
        let temp = fixture_dir();
        let db = temp.path().join("ledger.db");
        drop(Ledger::open(&db).expect("scratch ledger"));
        let state = test_state(temp.path(), MockEngine::available());
        let projects = format!("GET /api/hq/projects HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let load = format!("GET /api/hq/load HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let unheld = exchange(Arc::clone(&state), projects.as_bytes());
        assert_eq!(status(&unheld), 200, "{unheld}");
        assert_eq!(state.store_held_refusals.load(Ordering::Relaxed), 0);
        let before = std::fs::read(&db).expect("store bytes");

        let hold = thruline_core::store_hold::StoreHold::create(&db, "removal-01", "P1")
            .expect("hold scratch ledger");
        let refused = exchange(Arc::clone(&state), projects.as_bytes());
        assert_eq!(status(&refused), 503, "{refused}");
        assert_eq!(
            response_json(&refused),
            serde_json::json!({
                "error": "store_held",
                "message": "ThruLine is removing a project's records; try again when it finishes"
            })
        );
        assert_eq!(state.store_held_refusals.load(Ordering::Relaxed), 1);
        assert_eq!(std::fs::read(&db).expect("store bytes"), before);

        hold.release().expect("release hold");
        let resumed = exchange(Arc::clone(&state), projects.as_bytes());
        assert_eq!(status(&resumed), 200, "{resumed}");
        assert_eq!(resumed, unheld);
        let status_doc = response_json(&exchange(Arc::clone(&state), load.as_bytes()));
        assert_eq!(status_doc["store_held_refusals"], 1);
    }

    #[cfg(unix)]
    #[test]
    fn child_projection_routes_distinguish_a_hold_from_a_policy_exit_three() {
        let temp = fixture_dir();
        let db = temp.path().join("ledger.db");
        drop(Ledger::open(&db).expect("scratch ledger"));
        let executable = temp.path().join("held-cli");
        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s\\n' \"ThruLine is removing a project's records; try again when it finishes\" >&2\nexit 3\n",
        )
        .expect("fake held cli");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        let mut state = test_state(temp.path(), MockEngine::available());
        Arc::get_mut(&mut state).expect("sole owner").cli_exe = executable;
        let hold = thruline_core::store_hold::StoreHold::create(&db, "removal-01", "P1")
            .expect("hold scratch ledger");
        for path in [
            "/api/explain",
            "/api/phases?session=s",
            "/api/read?session=s",
        ] {
            let request = format!("GET {path} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
            let response = exchange(Arc::clone(&state), request.as_bytes());
            assert_eq!(status(&response), 503, "{path}: {response}");
            assert_eq!(response_json(&response)["error"], "store_held");
        }
        assert_eq!(state.store_held_refusals.load(Ordering::Relaxed), 3);
        hold.release().expect("release hold");
    }

    #[cfg(unix)]
    #[test]
    fn bounded_structured_child_keeps_only_the_exact_hold_signal() {
        use std::os::unix::fs::PermissionsExt;

        let temp = fixture_dir();
        let executable = temp.path().join("structured-cli");
        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s\\n' \"ThruLine is removing a project's records; try again when it finishes\" >&2\nexit 3\n",
        )
        .expect("fake held cli");
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
        let held =
            structured_projection(std::process::Command::new(&executable)).expect_err("held child");
        assert!(error_is_store_held(&held));

        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s' '{\"state\":\"ready\"}'\nprintf '%s' 'private/path' >&2\nexit 0\n",
        )
        .expect("fake unheld cli");
        let bytes =
            structured_projection(std::process::Command::new(&executable)).expect("unheld output");
        assert_eq!(bytes.as_slice(), br#"{"state":"ready"}"#);
    }

    fn valid_body() -> String {
        serde_json::json!({"project":PROJECT,"session":SESSION,"model":MODEL}).to_string()
    }

    fn framed(method: &str, path: &str, body: &[u8], extra_headers: &str) -> Vec<u8> {
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {ORIGIN}\r\n{extra_headers}Content-Length: {}\r\nContent-Type: application/json\r\n\r\n",
            body.len()
        );
        [head.as_bytes(), body].concat()
    }

    /// How long one read may sit silent before the harness looks up and asks
    /// whether the server is **wedged** rather than merely busy.
    ///
    /// **It is a poll interval, not a deadline** (R-2026-09-24-27). It used to
    /// be a deadline — one `read_to_string` under a five-second `SO_RCVTIMEO` —
    /// and that quietly made every test using `exchange` a measurement of how
    /// fast this machine can `fork`/`exec`. Three routes (`/api/explain`,
    /// `/api/sessions`, `/api/phases`) answer by running `state.cli_exe`
    /// through `Command::output()`, so the first response byte cannot exist
    /// until a child process has been spawned, run and reaped. Measured here
    /// with a cold `cargo build --release --workspace` running alongside, at
    /// `--test-threads=16`: that child takes up to 3.33 s, and past 5 s often
    /// enough to fail the suite — at which point the client read returned
    /// `EAGAIN` and the test panicked with a bare `WouldBlock`, every single
    /// time with `bytes = 0` and the handler thread still running. Nothing was
    /// contended and nothing was flaky: the harness had asked the machine to
    /// fork within five seconds, and the machine declined.
    const READ_POLL: Duration = Duration::from_millis(250);

    /// The point at which a silent server is called wedged instead of slow.
    ///
    /// This bound exists so a genuinely hung handler fails with a diagnosis
    /// rather than hanging the suite forever — the failure mode
    /// `tests/blocking_wait_census.rs` was written about. It is deliberately
    /// far above any spawn time load can produce, because it is not measuring
    /// speed: no amount of machine load turns a one-second request into a
    /// two-minute one, so crossing it means a handler that is never going to
    /// answer.
    const HANG_BOUND: Duration = Duration::from_secs(120);

    fn exchange(state: Arc<State>, request: &[u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            handle(&state, stream)
        });
        let mut client = TcpStream::connect(address).expect("connect");
        client
            .set_read_timeout(Some(READ_POLL))
            .expect("read timeout");
        client.write_all(request).expect("request");
        client.shutdown(Shutdown::Write).expect("shutdown write");

        // Read to EOF — every response carries `Connection: close` and the
        // handler drops the stream — treating a silent poll as "still working"
        // and only the wedge bound as a failure. The message distinguishes the
        // two cases, so a silent read can never again be reported as a bare
        // `WouldBlock` and written off as contention.
        let started = std::time::Instant::now();
        let mut bytes: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match std::io::Read::read(&mut client, &mut chunk) {
                Ok(0) => break,
                Ok(read) => bytes.extend_from_slice(&chunk[..read]),
                Err(err)
                    if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
                        && started.elapsed() < HANG_BOUND => {}
                Err(err) => panic!(
                    "the request was never answered: {err:?} after {:?}; \
                     {} response bytes arrived and the handler thread is {}. \
                     That is a wedged handler, not a slow machine — {HANG_BOUND:?} \
                     is far past any spawn time load can cause.",
                    started.elapsed(),
                    bytes.len(),
                    if server.is_finished() {
                        "finished, so something else is holding the connection open"
                    } else {
                        "still running"
                    }
                ),
            }
        }
        let response = String::from_utf8(bytes).expect("the response is UTF-8");
        server.join().expect("server thread").expect("handle");
        response
    }

    fn status(response: &str) -> u16 {
        response
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse().ok())
            .expect("HTTP status")
    }

    fn response_json(response: &str) -> serde_json::Value {
        let body = response.split("\r\n\r\n").nth(1).expect("body");
        serde_json::from_str(body).expect("JSON response")
    }

    /// A stand-in `thruline` that prints `body` and exits `code`.
    ///
    /// The projection's exit-code rule cannot be witnessed against the real CLI
    /// without a session that actually produces an unsupported claim, which
    /// needs a real test runner on the machine. Injecting the executable makes
    /// the rule testable on its own terms — which is the point of `cli_exe`
    /// being a field rather than a call to `current_exe` inside the handler.
    fn fake_cli(dir: &Path, name: &str, body: &str, code: i32) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!("#!/bin/sh\nprintf '%s' '{body}'\nexit {code}\n"),
        )
        .expect("fake cli");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        path
    }

    // -----------------------------------------------------------------------
    // T-2-1 — the two read-only Change routes.
    // -----------------------------------------------------------------------

    /// A store with one Change whose purpose carries something that looks like
    /// a credential, so the masking boundary is observable from the route.
    fn a_store_with_one_change(dir: &Path) -> String {
        use thruline_core::change::{
            self, AssociationBasis, AssociationProvenance, ChangeRequest, NewAssociation,
            NewChange, RequestSource,
        };
        let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id, root_path, source, display_name, created_at)
                 VALUES('cccccccc-0000-4000-8000-00000000p001', '/tmp/synthetic',
                        'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("project");
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id, project_id, file_path, started_at)
                 VALUES('cccccccc-0000-4000-8000-00000000s001',
                        'cccccccc-0000-4000-8000-00000000p001', '/tmp/synthetic/s.jsonl',
                        '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("session");
        let id = "cccccccc-0000-4000-8000-00000000c001".to_string();
        change::insert_change(
            &ledger,
            &NewChange {
                id: id.clone(),
                project_id: "cccccccc-0000-4000-8000-00000000p001".to_string(),
                opened_at: "2026-01-02T09:00:00Z".to_string(),
                request: ChangeRequest {
                    title: "Retry failed uploads".to_string(),
                    purpose: "Use AKIAIOSFODNN7EXAMPLE for the retry.".to_string(),
                    prompt_event_id: None,
                    requester: "sam".to_string(),
                    source: RequestSource::Person,
                },
            },
        )
        .expect("change");
        change::append_association(
            &ledger,
            &NewAssociation {
                id: "cccccccc-0000-4000-8000-00000000a001".to_string(),
                change_id: id.clone(),
                session_id: "cccccccc-0000-4000-8000-00000000s001".to_string(),
                provenance: AssociationProvenance::Suggested,
                basis: AssociationBasis::Timing,
                actor: None,
                asserted_at: "2026-01-02T10:00:00Z".to_string(),
                reason: None,
            },
        )
        .expect("association");
        id
    }

    #[test]
    fn the_changes_route_serves_the_record_and_the_words_for_it() {
        let temp = fixture_dir();
        let id = a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        // T-S-4 bumped the served Change shape to 3: `decisions` arrived.
        // T-4-1 bumped it to 4: `reviews` arrived.
        // STEP-7-GIT-LINE bumped it to 5: `code_moved` arrived.
        // QUAL-F2-R2 bumped it to 6: `retrospective` arrived.
        // C30 R-LEDGER bumped it to 7: `notes` arrived.
        // C30 R-LEDGER (seconds) bumped it to 8: `seconds` arrived.
        assert_eq!(body["schema_version"], 8);
        assert_eq!(body["changes"].as_array().map(|a| a.len()), Some(1));
        assert_eq!(body["changes"][0]["id"], id);
        // QUAL-F2-R2: served on every Change, `null` when the standing list
        // was not accepted after the work — the key the Q07 C8 probe reads.
        assert!(body["changes"][0]
            .as_object()
            .is_some_and(|c| c.contains_key("retrospective")));
        // The label a person reads comes from the copy table, in Sam's words.
        assert_eq!(
            body["labels"]["accepted_scope"]["word"],
            "what counts as done"
        );
        assert_eq!(body["labels"]["change"]["word"], "Change");
        // And the association is served as what it is, with no result on it.
        assert_eq!(
            body["changes"][0]["associations"][0]["provenance"],
            "suggested"
        );
        assert!(body["changes"][0]["associations"][0]
            .get("verdict")
            .is_none());
    }

    /// H's rule at the serving boundary: the text is masked before it is
    /// written to the socket, and the response says a masker ran.
    #[test]
    fn served_change_text_is_masked_before_it_reaches_the_socket() {
        let temp = fixture_dir();
        let id = a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/{id} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        assert!(
            !response.contains("AKIAIOSFODNN7EXAMPLE"),
            "raw stored text reached the socket: {response}"
        );
        let body = response_json(&response);
        assert_eq!(body["change"]["masking"]["state"], "ran");
        assert_eq!(body["change"]["examination"], "none_yet");
        assert_eq!(body["change"]["decision"], "none_yet");
    }

    /// A store with one Change and one confirmed conversation whose log holds
    /// `claim` as the agent's final message, ingested as events the way the
    /// ledger holds them. Returns the Change id and the log's path.
    fn a_change_with_one_conversation_logged(dir: &Path, claim: &str) -> (String, PathBuf) {
        let id = a_store_with_one_change(dir);
        let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
        let transcript = dir.join("agent-log.jsonl");
        let lines = [
            r#"{"type":"user","sessionId":"cccccccc-0000-4000-8000-00000000s001","message":{"role":"user","content":"Add a cart total function."}}"#.to_string(),
            serde_json::json!({
                "type": "assistant",
                "sessionId": "cccccccc-0000-4000-8000-00000000s001",
                "message": { "role": "assistant", "content": [{ "type": "text", "text": claim }] },
            })
            .to_string(),
        ];
        std::fs::write(&transcript, lines.join("\n") + "\n").expect("transcript");
        for (seq, kind) in [(1_i64, "user_msg"), (2, "assistant_text")] {
            ledger
                .conn()
                .execute(
                    "INSERT INTO events(id, project_id, session_id, seq, kind, raw_file,
                                        raw_line, raw_sha256, prev_hash, chain_hash)
                     VALUES(?1, 'cccccccc-0000-4000-8000-00000000p001',
                            'cccccccc-0000-4000-8000-00000000s001', ?2, ?3, ?4, ?2, ?5, '', '')",
                    rusqlite::params![
                        format!("ev-q{seq}"),
                        seq,
                        kind,
                        transcript.to_string_lossy(),
                        thruline_core::chain::sha256_hex(lines[(seq - 1) as usize].as_bytes()),
                    ],
                )
                .expect("event");
        }
        thruline_core::change::append_association(
            &ledger,
            &thruline_core::change::NewAssociation {
                id: "cccccccc-0000-4000-8000-00000000a002".to_string(),
                change_id: id.clone(),
                session_id: "cccccccc-0000-4000-8000-00000000s001".to_string(),
                provenance: thruline_core::change::AssociationProvenance::Confirmed,
                basis: thruline_core::change::AssociationBasis::User,
                actor: Some("sam".to_string()),
                asserted_at: "2026-01-02T11:00:00Z".to_string(),
                reason: None,
            },
        )
        .expect("confirm");
        (id, transcript)
    }

    /// `GET /api/changes/<id>` carries `agent_log` BESIDE the Change: for each
    /// confirmed conversation, the file and line where the agent's final
    /// message sits in its own log, or why there is none. Never the words. It
    /// is an additive field of the one route the Change screen already reads;
    /// the Change object itself, and its schema version, are unchanged.
    #[test]
    fn the_change_route_serves_where_the_agent_said_it_and_never_the_words() {
        let temp = fixture_dir();
        let (id, transcript) = a_change_with_one_conversation_logged(
            temp.path(),
            "Done. The cart total is added and the tests pass.",
        );

        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/{id} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        assert!(
            !response.contains("cart total is added"),
            "a word of the agent's log reached the socket: {response}"
        );
        let body = response_json(&response);
        assert!(body.get("agent_said").is_none(), "{body}");
        let entry = &body["agent_log"]["sessions"][0];
        assert_eq!(
            entry["session_id"], "cccccccc-0000-4000-8000-00000000s001",
            "{body}"
        );
        assert_eq!(
            entry["receipt"],
            serde_json::json!({ "file": transcript.to_string_lossy(), "line": 2 }),
            "{body}"
        );
        assert!(entry["not_located"].is_null(), "{body}");
        assert_eq!(body["agent_log"]["masking"]["state"], "ran", "{body}");
        // Beside the Change, never inside it; the Change's own shape is as it was.
        assert!(body["change"].get("agent_log").is_none(), "{body}");
        assert_eq!(
            body["schema_version"],
            thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION
        );
        // The read wrote nothing.
        let ledger =
            thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
        let rows: i64 = ledger
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM change_session_associations",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(rows, 2);
    }

    /// A claim that names a GitLab-shaped token reaches the socket no more
    /// than any other claim does: the place is served, and no word of the
    /// claim, credential or not.
    #[test]
    fn the_change_route_serves_no_word_of_a_claim_that_carries_a_credential() {
        let temp = fixture_dir();
        let (id, transcript) = a_change_with_one_conversation_logged(
            temp.path(),
            "Done. I set CI_TOKEN=glpat-AbCdEfGhIjKlMnOpQrSt in the pipeline config and the tests pass.",
        );

        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/{id} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        assert!(
            !response.contains("glpat-"),
            "the token reached the socket: {response}"
        );
        assert!(
            !response.contains("pipeline config"),
            "a word of the claim reached the socket: {response}"
        );
        let body = response_json(&response);
        assert_eq!(
            body["agent_log"]["sessions"][0]["receipt"],
            serde_json::json!({ "file": transcript.to_string_lossy(), "line": 2 }),
            "{body}"
        );
    }

    // -----------------------------------------------------------------------
    // AGENT-LOG-LOCATOR-BOUNDED — the Change route reads a log when it is
    // first looked at and again only when it changed.
    // -----------------------------------------------------------------------

    /// A store with one Change and one confirmed conversation whose log is
    /// `filler` lines of ordinary talk (about a kilobyte each) after the
    /// person's first message, then their last message and the agent's final
    /// claim, every line held in the ledger the way ingest holds it. Returns
    /// the Change id and the log's path. The final claim is on line
    /// `filler + 3`: the earlier talk is hashed and parsed, but it is not the
    /// final turn.
    fn a_change_with_a_large_log(dir: &Path, filler: usize) -> (String, PathBuf) {
        let id = a_store_with_one_change(dir);
        let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
        let transcript = dir.join("large-agent-log.jsonl");
        let session = "cccccccc-0000-4000-8000-00000000s001";
        let talk = "Reading the next module and thinking about how it fits. ".repeat(18);
        let mut lines = vec![serde_json::json!({
            "type": "user", "sessionId": session,
            "message": { "role": "user", "content": "Add a cart total function." },
        })
        .to_string()];
        for _ in 0..filler {
            lines.push(
                serde_json::json!({
                    "type": "assistant", "sessionId": session,
                    "message": { "role": "assistant", "content": [{ "type": "text", "text": talk }] },
                })
                .to_string(),
            );
        }
        lines.push(
            serde_json::json!({
                "type": "user", "sessionId": session,
                "message": { "role": "user", "content": "And the tests?" },
            })
            .to_string(),
        );
        lines.push(
            serde_json::json!({
                "type": "assistant", "sessionId": session,
                "message": { "role": "assistant", "content": [{ "type": "text",
                    "text": "Done. The cart total is added and the tests pass." }] },
            })
            .to_string(),
        );
        std::fs::write(&transcript, lines.join("\n") + "\n").expect("transcript");
        ledger.conn().execute_batch("BEGIN").expect("begin");
        for (n, text) in lines.iter().enumerate() {
            let seq = n as i64 + 1;
            ledger
                .conn()
                .execute(
                    "INSERT INTO events(id, project_id, session_id, seq, kind, raw_file,
                                        raw_line, raw_sha256, prev_hash, chain_hash)
                     VALUES(?1, 'cccccccc-0000-4000-8000-00000000p001', ?2, ?3, ?4, ?5, ?3, ?6,
                            '', '')",
                    rusqlite::params![
                        format!("ev-large-{seq}"),
                        session,
                        seq,
                        if n == 0 || n == lines.len() - 2 {
                            "user_msg"
                        } else {
                            "assistant_text"
                        },
                        transcript.to_string_lossy(),
                        thruline_core::chain::sha256_hex(text.as_bytes()),
                    ],
                )
                .expect("event");
        }
        ledger.conn().execute_batch("COMMIT").expect("commit");
        thruline_core::change::append_association(
            &ledger,
            &thruline_core::change::NewAssociation {
                id: "cccccccc-0000-4000-8000-00000000a003".to_string(),
                change_id: id.clone(),
                session_id: session.to_string(),
                provenance: thruline_core::change::AssociationProvenance::Confirmed,
                basis: thruline_core::change::AssociationBasis::User,
                actor: Some("sam".to_string()),
                asserted_at: "2026-01-02T11:00:00Z".to_string(),
                reason: None,
            },
        )
        .expect("confirm");
        (id, transcript)
    }

    /// Asking again about a log nobody touched does not read it again: the
    /// route's answer rests on the file's identity and on what the ledger
    /// holds, and the same file under the same ledger is the same answer.
    #[test]
    fn the_change_route_does_not_read_an_unchanged_log_again() {
        let temp = fixture_dir();
        let (id, transcript) = a_change_with_one_conversation_logged(
            temp.path(),
            "Done. The cart total is added and the tests pass.",
        );
        let state = test_state(temp.path(), MockEngine::available());
        let get = || {
            exchange(
                state.clone(),
                format!("GET /api/changes/{id} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            )
        };
        let first = response_json(&get());
        assert_eq!(first["agent_log"]["sessions"][0]["receipt"]["line"], 2);
        assert_eq!(thruline_core::agent_log::reads_of(&transcript), 1);
        for _ in 0..3 {
            assert_eq!(response_json(&get())["agent_log"], first["agent_log"]);
        }
        assert_eq!(
            thruline_core::agent_log::reads_of(&transcript),
            1,
            "an unchanged log was read again"
        );

        // A log that changed is read again, and what it says now is served.
        let mut more = std::fs::read_to_string(&transcript).expect("read");
        more.push_str(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"More?\"}}\n",
        );
        std::fs::write(&transcript, more).expect("append");
        assert_eq!(
            response_json(&get())["agent_log"]["sessions"][0]["receipt"]["line"],
            2,
            "a line the ledger does not hold moved the place"
        );
        assert_eq!(thruline_core::agent_log::reads_of(&transcript), 2);
    }

    /// A very large log costs its one read, not one per request: the route's
    /// answer time does not grow with asking again.
    #[test]
    fn the_change_route_reads_a_very_large_log_once_not_on_every_request() {
        const FILLER: usize = 30_000;
        let temp = fixture_dir();
        let (id, transcript) = a_change_with_a_large_log(temp.path(), FILLER);
        let state = test_state(temp.path(), MockEngine::available());
        let get = || {
            let started = std::time::Instant::now();
            let response = exchange(
                state.clone(),
                format!("GET /api/changes/{id} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            );
            assert_eq!(status(&response), 200, "{response}");
            (started.elapsed(), response_json(&response))
        };
        let (first_time, first) = get();
        assert_eq!(
            first["agent_log"]["sessions"][0]["receipt"]["line"],
            FILLER + 3,
            "{first}"
        );
        assert_eq!(thruline_core::agent_log::reads_of(&transcript), 1);

        let mut later = std::time::Duration::ZERO;
        for _ in 0..5 {
            let (time, body) = get();
            assert_eq!(body["agent_log"], first["agent_log"]);
            later += time;
        }
        assert_eq!(
            thruline_core::agent_log::reads_of(&transcript),
            1,
            "a very large log was read again on a later request"
        );
        eprintln!(
            "a {FILLER}-line log: the first request took {first_time:?}; five more took {later:?}"
        );
        assert!(
            later < first_time,
            "five more requests took {later:?}; the first, which read the log, took {first_time:?}"
        );
    }

    // -----------------------------------------------------------------------
    // T-3-1 — the coverage route and the one link act.
    // -----------------------------------------------------------------------

    /// A store with one Change, one confirmed in-scope piece of work, and two
    /// recorded runs: one that settles something and one that settles nothing.
    fn a_store_with_coverage(dir: &Path) -> String {
        let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
        let conn = ledger.conn();
        let project = "cccccccc-0000-4000-8000-00000000p001";
        let session = "cccccccc-0000-4000-8000-00000000s001";
        let change = "cccccccc-0000-4000-8000-00000000c001";
        // The project's working folder is a real folder on this machine, and
        // the store says how it was established (T-3-2b): a changed path that
        // is absolute only becomes scope under a root the capture named, so a
        // fixture whose root is `/tmp/synthetic` has no established root and
        // every path in it abstains. That is the behaviour, not the fixture's
        // business to route around, so the fixture gives it a real one.
        let root = dir
            .canonicalize()
            .unwrap_or_else(|_| dir.to_path_buf())
            .display()
            .to_string();
        conn.execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1,?2,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
            [project, root.as_str()],
        )
        .expect("project");
        conn.execute(
            "INSERT INTO workspace_identities(project_id, repo_id, basis, resolved_root,
                 resolved_via, sessions_sampled, observed_at)
             VALUES(?1,'repo','git',?2,'recorded_cwd',1,'2026-01-01T00:00:00Z')",
            [project, root.as_str()],
        )
        .expect("workspace identity");
        conn.execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1,?2,'/tmp/synthetic/s.jsonl','2026-01-01T00:00:00Z')",
            [session, project],
        )
        .expect("session");
        conn.execute(
            &format!(
                "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                     tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                 VALUES('cccccccc-0000-4000-8000-00000000e001',?1,?2,1,'2026-01-01T00:10:00Z',
                        'tool_use','Edit',
                        '{{\"file_path\":\"{root}/crates/core/src/importer.rs\"}}',
                        '/tmp/synthetic/s.jsonl',1,'aa','','bb')"
            ),
            [project, session],
        )
        .expect("event");
        conn.execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES(?1,?2,'2026-01-02T09:00:00Z','Keep malformed lines',
                    'Use AKIAIOSFODNN7EXAMPLE for the retry.','sam','person')",
            [change, project],
        )
        .expect("change");
        conn.execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES(?1,1,'2026-01-02T09:30:00Z',
                    '[\"The build stays clean under clippy after the change.\",
                      \"HQ reopens where you left it after a restart, using AKIAIOSFODNN7EXAMPLE.\"]',
                    '[]','[\"crates/**\"]','[]','[\"clippy\"]','digest')",
            [change],
        )
        .expect("scope");
        conn.execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                 accepted_at, rationale)
             VALUES(?1,1,'sam','2026-01-02T09:40:00Z','this is what counts')",
            [change],
        )
        .expect("acceptance");
        conn.execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
                 basis, actor, asserted_at, reason)
             VALUES('cccccccc-0000-4000-8000-00000000a001',?1,?2,'confirmed','user','sam',
                    '2026-01-02T10:00:00Z','mine')",
            [change, session],
        )
        .expect("association");
        let captured = "{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":9}";
        conn.execute(
            &format!(
                "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                     scope_key, root, repository_identity, pre_identity, post_identity,
                     exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
                 VALUES('cccccccc-0000-4000-8000-00000000b001',0,'2026-01-03T09:00:00Z',1,
                        'build','bash:/tmp/synthetic:cargo clippy','/tmp/synthetic','repo',
                        'pre','post',0,NULL,'2026-01-03T09:00:00Z',?1,
                        '{{\"stdout\":{captured},\"stderr\":{captured}}}','ddd')"
            ),
            [session],
        )
        .expect("binding");
        change.to_string()
    }

    #[test]
    fn the_coverage_route_says_what_the_record_holds_for_each_accepted_line() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/{id}/coverage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        assert_eq!(body["schema_version"], 1);
        let criteria = body["coverage"]["criteria"].as_array().expect("criteria");
        assert_eq!(criteria.len(), 2);
        // The line about the build has the run that settles something under it.
        assert_eq!(criteria[0]["state"], "evidence");
        assert_eq!(criteria[0]["items"][0]["basis"], "declared");
        // The restart line has nothing, and says so in Sam's words.
        assert_eq!(criteria[1]["state"], "none");
        assert_eq!(criteria[1]["state_slug"], "coverage_no_test_ran");
        assert_eq!(
            body["lines"]["coverage_no_test_ran"]["plain"],
            "No test ran for this."
        );
    }

    /// The route derives before it serves, and deriving twice writes nothing
    /// new: two reads in a row serve the same document.
    #[test]
    fn reading_the_coverage_twice_serves_the_same_document() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());
        let request = format!("GET /api/changes/{id}/coverage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let first = response_json(&exchange(
            test_state(temp.path(), MockEngine::available()),
            request.as_bytes(),
        ));
        let second = response_json(&exchange(
            test_state(temp.path(), MockEngine::available()),
            request.as_bytes(),
        ));
        assert_eq!(first["coverage"], second["coverage"]);
        assert_eq!(second["derived"]["inserted"], 0, "a replay writes nothing");
    }

    /// T-4-1 — a store with one Change, one version, and one complete review
    /// of it whose raw report carries a credential the masker must catch.
    fn a_store_with_a_review(dir: &Path) -> (String, String) {
        let change = a_store_with_coverage(dir);
        let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO examination_inputs(id,subject_kind,subject_id,recorded_at,
                     schema_version,base_revision,uncommitted_state,config_files_json,
                     tools_json,target_kind)
                 VALUES('exi-review','intended',?1,'2026-01-03T10:00:00Z',1,
                     'abcdef1234567890abcdef1234567890abcdef12','clean','[]','[]','none')",
                [&change],
            )
            .expect("version");
        let review = "review-t41-route";
        thruline_core::change_review::append_review(
            &ledger,
            &thruline_core::change_review::NewReview {
                id: review.into(),
                change_id: change.clone(),
                review_round: 1,
                reviewer_identity: "PR-Agent 0.45.0 via qwen3:8b".into(),
                reviewer_type: thruline_core::change_review::ReviewerType::Model,
                review_source: "local/plain-diff".into(),
                reviewer_relationship: "not established".into(),
                relationship_provenance: "fixture".into(),
                target_revision: "abcdef1234567890abcdef1234567890abcdef12".into(),
                scope_version: Some(1),
                examined_inputs_id: "exi-review".into(),
                diff_sha256: "d".repeat(64),
                route_identity_json: serde_json::json!({
                    "endpoint": thruline_core::change_review::ROUTE_ENDPOINT,
                    "model": "qwen3:8b",
                    "model_manifest_sha256": "5".repeat(64),
                    "runtime_sha256": "4".repeat(64),
                    "provider_source_commit": thruline_core::change_review::PR_AGENT_SOURCE_COMMIT,
                    "image_id": format!("sha256:{}", "9".repeat(64)),
                    "plan_digest": "e".repeat(64),
                })
                .to_string(),
                started_at: "2026-01-03T10:01:00Z".into(),
                ended_at: Some("2026-01-03T10:02:00Z".into()),
                state: thruline_core::change_review::ReviewState::Complete,
                recommendation: None,
                limitations: vec!["advisory".into()],
                raw_bytes: br#"{"note":"key AKIAIOSFODNN7EXAMPLE in the log"}"#.to_vec(),
                parser_id: "thruline.pr-agent-structured.v1".into(),
                parsed_json: r#"{"state":"complete"}"#.into(),
                parse_complete: true,
                findings: vec![thruline_core::change_review::ReviewFinding {
                    kind: "Addition returns subtraction".into(),
                    message: "The operator subtracts.".into(),
                    path: Some("calculator.py".into()),
                    line: Some(2),
                }],
            },
        )
        .expect("review");
        (change, review.to_string())
    }

    fn get(dir: &Path, path: &str) -> String {
        exchange(
            test_state(dir, MockEngine::available()),
            format!("GET {path} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        )
    }

    /// R12 — the Change document carries the review with its typed count and
    /// its version; the drill-down serves a masked copy only.
    #[test]
    fn the_change_carries_its_review_and_the_drill_down_is_masked() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        let body = response_json(&get(temp.path(), &format!("/api/changes/{change}")));
        let r = &body["change"]["reviews"][0];
        assert_eq!(r["finding_count"], 1);
        assert_eq!(r["applies_to_current_version"], true);
        assert_eq!(r["integrity"], "intact");
        assert_eq!(r["state"], "complete");
        assert!(
            r.get("raw_bytes").is_none(),
            "raw bytes are never in the Change view"
        );

        let raw = get(
            temp.path(),
            &format!("/api/changes/{change}/reviews/{review}/raw"),
        );
        assert_eq!(status(&raw), 200, "{raw}");
        assert!(
            !raw.contains("AKIAIOSFODNN7EXAMPLE"),
            "a credential reached the socket"
        );
        let raw = response_json(&raw);
        assert_eq!(raw["artifact"], "raw");
        assert_ne!(raw["original_sha256"], raw["derivative_sha256"]);
        let parsed = get(
            temp.path(),
            &format!("/api/changes/{change}/reviews/{review}/parsed"),
        );
        assert_eq!(status(&parsed), 200, "{parsed}");
        let foreign = get(
            temp.path(),
            &format!("/api/changes/chg-other/reviews/{review}/raw"),
        );
        assert_eq!(status(&foreign), 404);
        assert_eq!(response_json(&foreign)["error"], "no_such_review");
    }

    // -----------------------------------------------------------------------
    // T-4-2 — where each thing a review found stands: one read, one write.
    // -----------------------------------------------------------------------

    fn standing_post(dir: &Path, path: &str, body: serde_json::Value, origin: bool) -> String {
        let headers = if origin {
            format!("Origin: http://{ORIGIN}\r\n")
        } else {
            String::new()
        };
        exchange(
            test_state(dir, MockEngine::available()),
            &framed("POST", path, body.to_string().as_bytes(), &headers),
        )
    }

    fn standing_rows(dir: &Path) -> i64 {
        let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
        ledger
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM change_review_finding_standings",
                [],
                |r| r.get(0),
            )
            .expect("count")
    }

    /// The read: every finding, open with no decision recorded until someone
    /// decides; an unknown Change is a 404, not an empty list.
    #[test]
    fn the_findings_route_serves_each_finding_and_where_it_stands() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        let response = get(temp.path(), &format!("/api/changes/{change}/findings"));
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        assert_eq!(body["schema_version"], 2);
        let f = &body["findings"]["findings"][0];
        assert_eq!(f["key"], format!("{review}:0"));
        assert_eq!(f["standing"], "open");
        assert_eq!(f["decided"], false);
        assert_eq!(f["group"], "still_wrong");
        assert_eq!(body["findings"]["summary"]["still_wrong"], 1);
        let missing = get(temp.path(), "/api/changes/chg-nope/findings");
        assert_eq!(status(&missing), 404);
        assert_eq!(standing_rows(temp.path()), 0, "reading writes nothing");
    }

    /// The write needs the same origin, like the other five.
    #[test]
    fn the_standing_route_needs_the_same_origin() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        let path = format!("/api/changes/{change}/findings/{review}%3A0/standing");
        let body = serde_json::json!({"to": "under_repair", "actor": "sam"});
        let response = standing_post(temp.path(), &path, body, false);
        assert_eq!(status(&response), 403, "{response}");
        assert_eq!(standing_rows(temp.path()), 0);
    }

    /// Setting a finding aside without a reason and an authority is refused
    /// with the parts that are missing; with them it is saved, once.
    #[test]
    fn a_waiver_needs_a_reason_and_an_authority_through_the_route_too() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        let path = format!("/api/changes/{change}/findings/{review}%3A0/standing");
        let bare = standing_post(
            temp.path(),
            &path,
            serde_json::json!({"to": "waived", "actor": "sam"}),
            true,
        );
        assert_eq!(status(&bare), 409, "{bare}");
        let refused = response_json(&bare);
        assert_eq!(refused["error"], "missing");
        assert_eq!(
            refused["missing"],
            serde_json::json!(["reason", "authority"])
        );
        assert_eq!(standing_rows(temp.path()), 0);

        let full = standing_post(
            temp.path(),
            &path,
            serde_json::json!({
                "to": "waived", "actor": "sam", "authority": "I own this project",
                "reason": "The subtraction is what the client asked for.", "seen": "open",
            }),
            true,
        );
        assert_eq!(status(&full), 200, "{full}");
        let saved = response_json(&full);
        assert_eq!(saved["acted"]["outcome"], "recorded");
        assert_eq!(saved["findings"]["findings"][0]["standing"], "waived");
        assert_eq!(saved["findings"]["findings"][0]["group"], "set_aside");
        assert_eq!(saved["findings"]["summary"]["fixed"], 0);
        assert_eq!(standing_rows(temp.path()), 1);
    }

    /// ADV-review-input-1 (adversary F3): the adversary's own body — a
    /// waiver whose name, authority and reason are each one zero-width space
    /// — is refused by name through the screen's route, and nothing is written.
    #[test]
    fn a_waiver_with_an_invisible_name_authority_and_reason_is_refused_through_the_route() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        let path = format!("/api/changes/{change}/findings/{review}%3A0/standing");
        let invisible = standing_post(
            temp.path(),
            &path,
            serde_json::json!({
                "to": "waived", "actor": "\u{200B}", "authority": "\u{200B}",
                "reason": "\u{200B}", "seen": "open",
            }),
            true,
        );
        assert_eq!(status(&invisible), 409, "{invisible}");
        let refused = response_json(&invisible);
        assert_eq!(refused["error"], "missing");
        assert_eq!(
            refused["missing"],
            serde_json::json!(["actor", "reason", "authority"])
        );
        assert_eq!(standing_rows(temp.path()), 0);
    }

    /// ADV-review-input-4 (the pre-gate's variant of F3): parts that hold no
    /// letter, number, punctuation or symbol a person can read are refused by
    /// name through the screen's route, and nothing is written — the
    /// interlinear annotation characters U+FFF9–U+FFFB, which Chrome draws with
    /// no ink, and the pre-gate's own body, whose authority was U+FFF9 and
    /// whose reason was one combining accent with nothing under it (200 on
    /// the candidate it measured).
    #[test]
    fn a_waiver_whose_parts_hold_nothing_readable_is_refused_through_the_route() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        let path = format!("/api/changes/{change}/findings/{review}%3A0/standing");
        let annotation = standing_post(
            temp.path(),
            &path,
            serde_json::json!({
                "to": "waived", "actor": "\u{FFFA}", "authority": "\u{FFF9}",
                "reason": "\u{FFF9}\u{FFFB}", "seen": "open",
            }),
            true,
        );
        assert_eq!(status(&annotation), 409, "{annotation}");
        let refused = response_json(&annotation);
        assert_eq!(refused["error"], "missing");
        assert_eq!(
            refused["missing"],
            serde_json::json!(["actor", "reason", "authority"])
        );

        let accent = standing_post(
            temp.path(),
            &path,
            serde_json::json!({
                "to": "disputed", "actor": "Sam", "authority": "\u{FFF9}",
                "reason": "\u{0301}", "seen": "open",
            }),
            true,
        );
        assert_eq!(status(&accent), 409, "{accent}");
        let refused = response_json(&accent);
        assert_eq!(refused["error"], "missing");
        assert_eq!(
            refused["missing"],
            serde_json::json!(["reason", "authority"])
        );
        assert_eq!(standing_rows(temp.path()), 0);
    }

    /// Every character of `text` as JSON's `\uXXXX` escape, the longest way
    /// JSON can write it: a character outside the Basic Multilingual Plane is
    /// a surrogate pair, twelve bytes for one character.
    fn json_escaped(text: &str) -> String {
        text.encode_utf16().map(|u| format!("\\u{u:04x}")).collect()
    }

    /// ADV-review-input-2 (adversary F4): the screen's route and the terminal
    /// door (`change_finding_cli.rs`,
    /// `a_reason_over_two_thousand_characters_is_refused_from_a_terminal_as_it_is_from_the_screen`)
    /// hold one limit: 2,000 characters for each typed part. What a person can
    /// save from a terminal is saved through the route — the adversary's
    /// 436-character reason, a 140-character CJK one, and three parts at the
    /// limit written the longest way JSON allows — and a part over it is
    /// refused `too_long` by name. A body larger than any three parts at the
    /// limit could make is refused with the same code, never as malformed.
    #[test]
    fn a_long_reason_is_refused_and_saved_by_the_same_rule_as_the_terminal() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        let path = format!("/api/changes/{change}/findings/{review}%3A0/standing");

        let ascii: String = "The wording is what the client asked for. "
            .repeat(11)
            .chars()
            .take(436)
            .collect();
        let saved = standing_post(
            temp.path(),
            &path,
            serde_json::json!({
                "to": "waived", "actor": "sam", "authority": "I own this project",
                "reason": ascii, "seen": "open",
            }),
            true,
        );
        assert_eq!(status(&saved), 200, "{saved}");
        assert_eq!(standing_rows(temp.path()), 1);

        let cjk = standing_post(
            temp.path(),
            &path,
            serde_json::json!({
                "to": "disputed", "actor": "sam", "authority": "I own this project",
                "reason": "\u{8A9E}".repeat(140), "seen": "waived",
            }),
            true,
        );
        assert_eq!(status(&cjk), 200, "{cjk}");
        assert_eq!(standing_rows(temp.path()), 2);

        let at = json_escaped(&"\u{1F600}".repeat(2000));
        let widest = format!(
            r#"{{"to":"informational","actor":"{at}","authority":"{at}","reason":"{at}","seen":"disputed"}}"#
        );
        let response = exchange(
            test_state(temp.path(), MockEngine::available()),
            &framed(
                "POST",
                &path,
                widest.as_bytes(),
                &format!("Origin: http://{ORIGIN}\r\n"),
            ),
        );
        assert_eq!(status(&response), 200, "{} bytes", widest.len());
        assert_eq!(standing_rows(temp.path()), 3);

        let over = standing_post(
            temp.path(),
            &path,
            serde_json::json!({
                "to": "waived", "actor": "sam", "authority": "I own this project",
                "reason": "\u{8A9E}".repeat(2001), "seen": "informational",
            }),
            true,
        );
        assert_eq!(status(&over), 409, "{over}");
        let refused = response_json(&over);
        assert_eq!(refused["error"], "too_long");
        assert_eq!(refused["too_long"], serde_json::json!(["reason"]));
        assert_eq!(standing_rows(temp.path()), 3, "nothing over the limit");

        let oversize = format!(
            "POST {path} HTTP/1.1\r\nHost: {ORIGIN}\r\nOrigin: http://{ORIGIN}\r\nContent-Length: 1000000\r\nContent-Type: application/json\r\n\r\n"
        );
        let answer = exchange(
            test_state(temp.path(), MockEngine::available()),
            oversize.as_bytes(),
        );
        assert_eq!(status(&answer), 413, "{answer}");
        assert_eq!(response_json(&answer)["error"], "too_long");
        assert_eq!(standing_rows(temp.path()), 3);
    }

    /// A resolved mark from the screen reaches "waiting for a re-check" and
    /// never "fixed"; the synthetic import cannot be named from a browser.
    #[test]
    fn a_resolved_mark_from_the_screen_is_never_a_fix() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        let path = format!("/api/changes/{change}/findings/{review}%3A0/standing");
        let too_far = standing_post(
            temp.path(),
            &path,
            serde_json::json!({"to": "fixed_with_evidence", "resolved_mark": true, "actor": "sam",
                "evidence": {"kind": "review", "id": review}}),
            true,
        );
        assert_eq!(status(&too_far), 409, "{too_far}");
        assert_eq!(response_json(&too_far)["error"], "mark_goes_no_further");
        let synthetic = standing_post(
            temp.path(),
            &path,
            serde_json::json!({"to": "awaiting_verification", "via": "synthetic_resolved_import", "actor": "sam"}),
            true,
        );
        assert_eq!(status(&synthetic), 400, "{synthetic}");
        let unmarked = standing_post(
            temp.path(),
            &path,
            serde_json::json!({"to": "awaiting_verification", "actor": "sam"}),
            true,
        );
        assert_eq!(status(&unmarked), 409);
        assert_eq!(standing_rows(temp.path()), 0);
        let marked = standing_post(
            temp.path(),
            &path,
            serde_json::json!({"to": "awaiting_verification", "resolved_mark": true, "actor": "sam"}),
            true,
        );
        assert_eq!(status(&marked), 200, "{marked}");
        let f = &response_json(&marked)["findings"]["findings"][0];
        assert_eq!(f["standing"], "awaiting_verification");
        assert_eq!(f["counts_as_fixed"], false);
        assert_eq!(f["latest"]["via"], "resolved_mark");
        // A fix with no evidence is refused by name.
        let fixed = standing_post(
            temp.path(),
            &path,
            serde_json::json!({"to": "fixed_with_evidence", "actor": "sam"}),
            true,
        );
        assert_eq!(response_json(&fixed)["error"], "evidence_required");
        assert_eq!(standing_rows(temp.path()), 1);
    }

    /// R08 — altered bytes: the drill-down refuses with a code, and the
    /// Change stays readable and says the review is altered.
    #[test]
    fn an_altered_review_is_refused_below_and_named_above() {
        let temp = fixture_dir();
        let (change, review) = a_store_with_a_review(temp.path());
        {
            let ledger =
                thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
            ledger
                .conn()
                .execute_batch(
                    "DROP TRIGGER change_review_raw_artifacts_no_update;
                     DROP TRIGGER change_review_raw_artifacts_no_delete;
                     UPDATE change_review_raw_artifacts SET raw_bytes=CAST('{}' AS BLOB), byte_length=2;",
                )
                .expect("tamper");
        }
        let raw = get(
            temp.path(),
            &format!("/api/changes/{change}/reviews/{review}/raw"),
        );
        assert_eq!(status(&raw), 409);
        assert_eq!(response_json(&raw)["error"], "review_integrity_altered");
        let one = get(temp.path(), &format!("/api/changes/{change}"));
        assert_eq!(
            status(&one),
            200,
            "one damaged review never blanks the Change"
        );
        let r = &response_json(&one)["change"]["reviews"][0];
        assert_eq!(r["integrity"], "altered");
        assert_eq!(r["applies_to_current_version"], false);
        assert_eq!(r["finding_count"], 0);
        {
            let ledger =
                thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
            ledger
                .conn()
                .execute_batch("DELETE FROM change_review_raw_artifacts;")
                .expect("remove");
        }
        let gone = get(
            temp.path(),
            &format!("/api/changes/{change}/reviews/{review}/raw"),
        );
        assert_eq!(status(&gone), 409);
        assert_eq!(response_json(&gone)["error"], "review_integrity_missing");
    }

    #[test]
    fn served_coverage_text_is_masked_before_it_reaches_the_socket() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/{id}/coverage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert!(
            !response.contains("AKIAIOSFODNN7EXAMPLE"),
            "raw stored text reached the socket"
        );
        let body = response_json(&response);
        assert_eq!(body["coverage"]["masking"]["state"], "ran");
    }

    #[test]
    fn a_link_act_without_a_name_is_refused_in_the_copy_files_words() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/links"),
                br#"{"criterion":2,"kind":"check_run","evidence":"cccccccc-0000-4000-8000-00000000b001","act":"link"}"#,
                &format!("Origin: http://{ORIGIN}\r\n"),
            ),
        );
        assert_eq!(status(&response), 400, "{response}");
        let body = response_json(&response);
        assert_eq!(body["error"], "actor_required");
        assert_eq!(
            body["plain"],
            "We cannot add or remove a link without a name. Say who is doing it."
        );
    }

    #[test]
    fn a_link_act_needs_the_same_origin_like_every_other_write() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            &framed("POST", &format!("/api/changes/{id}/links"), b"{}", ""),
        );
        assert_eq!(status(&response), 403, "{response}");
    }

    /// Link, then unlink: the second act appends a tombstone and the first row
    /// is still counted. Nothing is deleted at any point.
    #[test]
    fn linking_and_unlinking_both_append_and_the_screen_follows() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());
        let post = |body: &str| {
            exchange(
                test_state(temp.path(), MockEngine::available()),
                &framed(
                    "POST",
                    &format!("/api/changes/{id}/links"),
                    body.as_bytes(),
                    &format!("Origin: http://{ORIGIN}\r\n"),
                ),
            )
        };
        let read = || {
            response_json(&exchange(
                test_state(temp.path(), MockEngine::available()),
                format!("GET /api/changes/{id}/coverage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n")
                    .as_bytes(),
            ))
        };
        // The route derives first, so read once before acting.
        let before = read();
        assert_eq!(before["coverage"]["criteria"][1]["state"], "none");

        let linked = post(
            r#"{"criterion":2,"kind":"check_run","evidence":"cccccccc-0000-4000-8000-00000000b001","act":"link","actor":"sam","reason":"I checked by hand"}"#,
        );
        assert_eq!(status(&linked), 200, "{linked}");
        let after_link = read();
        assert_eq!(after_link["coverage"]["criteria"][1]["state"], "evidence");
        assert_eq!(
            after_link["coverage"]["criteria"][1]["items"][0]["basis"],
            "manual"
        );
        assert_eq!(
            after_link["coverage"]["criteria"][1]["items"][0]["actor"],
            "sam"
        );
        let rows_after_link = after_link["coverage"]["link_rows"].as_u64().expect("rows");

        let unlinked = post(
            r#"{"criterion":2,"kind":"check_run","evidence":"cccccccc-0000-4000-8000-00000000b001","act":"unlink","actor":"sam","reason":"wrong line"}"#,
        );
        assert_eq!(status(&unlinked), 200, "{unlinked}");
        let after_unlink = read();
        assert_eq!(after_unlink["coverage"]["criteria"][1]["state"], "none");
        assert_eq!(
            after_unlink["coverage"]["criteria"][1]["removed"]
                .as_array()
                .map(|a| a.len()),
            Some(1),
            "unlinking never deletes"
        );
        assert_eq!(
            after_unlink["coverage"]["link_rows"].as_u64(),
            Some(rows_after_link + 1)
        );
    }

    /// T-3-2 — the inputs route serves what will be examined, in both
    /// registers, and writes nothing.
    #[test]
    fn the_inputs_route_says_what_will_be_examined_and_writes_nothing() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());

        // Before anything is written down, the route says so rather than
        // guessing a version.
        let request = format!("GET /api/changes/{id}/inputs HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let before = exchange(
            test_state(temp.path(), MockEngine::available()),
            request.as_bytes(),
        );
        assert_eq!(status(&before), 200, "{before}");
        let body = response_json(&before);
        assert_eq!(body["schema_version"], 1);
        assert_eq!(body["prepare"]["inputs"], serde_json::Value::Null);
        assert_eq!(
            body["prepare"]["lines"][0]["slug"],
            "prepare_nothing_recorded"
        );
        assert_eq!(
            body["lines"]["inputs_other_app"]["plain"],
            "This came from a different running copy of the app."
        );

        // One record, written by the engine as `change prepare` would.
        {
            let ledger =
                thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("open");
            let mut inputs = thruline_core::examined_inputs::ExaminedInputs::unknown_for(
                thruline_core::examined_inputs::SubjectKind::Intended,
                &id,
            );
            inputs.base_revision = thruline_core::examined_inputs::Input::known("abc123");
            inputs.source_identity = thruline_core::examined_inputs::Input::known("wsid1:aaa");
            inputs.worktree_path = thruline_core::examined_inputs::Input::known("/tmp/synthetic");
            inputs.uncommitted = thruline_core::examined_inputs::Uncommitted::Clean;
            thruline_core::examined_inputs::record(&ledger, &inputs, "2026-01-03T09:00:00Z")
                .expect("record");
        }

        let after = response_json(&exchange(
            test_state(temp.path(), MockEngine::available()),
            request.as_bytes(),
        ));
        let slugs: Vec<&str> = after["prepare"]["lines"]
            .as_array()
            .expect("lines")
            .iter()
            .filter_map(|l| l["slug"].as_str())
            .collect();
        assert_eq!(
            slugs,
            vec![
                "prepare_repository",
                "prepare_base",
                "prepare_uncommitted_none"
            ]
        );
        assert_eq!(after["prepare"]["record_count"], 1);

        // Reading it twice serves the same document and writes nothing.
        let again = response_json(&exchange(
            test_state(temp.path(), MockEngine::available()),
            request.as_bytes(),
        ));
        assert_eq!(after["prepare"], again["prepare"]);
        assert_eq!(again["prepare"]["record_count"], 1);
    }

    #[test]
    fn the_inputs_route_answers_404_for_a_change_that_does_not_exist() {
        let temp = fixture_dir();
        a_store_with_coverage(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/nope/inputs HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 404, "{response}");
    }

    #[test]
    fn the_coverage_route_answers_404_for_a_change_that_does_not_exist() {
        let temp = fixture_dir();
        a_store_with_coverage(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/nope/coverage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 404, "{response}");
    }

    /// T-S-2 — the applicability route serves an empty item list, and both
    /// registers, for a Change nothing has been assessed against yet, and
    /// answers 404 for a Change that does not exist.
    #[test]
    fn the_applicability_route_serves_an_empty_list_before_anything_is_assessed() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());

        let request =
            format!("GET /api/changes/{id}/applicability HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let response = exchange(
            test_state(temp.path(), MockEngine::available()),
            request.as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        // 2 since APPLICABILITY-SCHEMA-VERSION; this route's document gained
        // `checked_paths` at T-F-1-F1 and the number had not followed it.
        assert_eq!(body["schema_version"], 2);
        assert_eq!(body["items"].as_array().map(|a| a.len()), Some(0));
        assert_eq!(
            body["lines"]["applicability_needs_a_recheck"]["plain"],
            "Proven on the previous version — needs a re-check."
        );
    }

    /// APPLICABILITY-SCHEMA-VERSION — the applicability document says it is
    /// version 2, and an item carries the field version 2 is named for.
    ///
    /// `checked_paths` was added to every item at T-F-1-F1 while the document
    /// still called itself version 1. Nothing broke, because nothing branches
    /// on the number — which is exactly why a wrong one survives unnoticed:
    /// the next reader has no way to tell a document with the field from one
    /// without it. This test fails if the number and the shape part company
    /// again, in either direction.
    #[test]
    fn the_applicability_document_declares_schema_version_2_and_carries_checked_paths() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());
        {
            let ledger =
                thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
            ledger
                .conn()
                .execute(
                    "INSERT INTO change_criterion_evidence(id, change_id, scope_version,
                         criterion_index, evidence_kind, evidence_id, session_id, basis,
                         detail, actor, asserted_at)
                     VALUES('cccccccc-0000-4000-8000-00000000l009',?1,1,1,'check_run','run9',
                            'cccccccc-0000-4000-8000-00000000s001','declared','clippy',NULL,
                            '2026-01-04T09:00:00Z')",
                    [id.as_str()],
                )
                .expect("link");
            ledger
                .conn()
                .execute(
                    "INSERT INTO change_criterion_freshness(id, change_id, criterion_evidence_id,
                         criterion_index, scope_version, old_snapshot, new_snapshot, state,
                         reason_class, reason_paths_json, recorded_at)
                     VALUES('cccccccc-0000-4000-8000-00000000f009',?1,
                            'cccccccc-0000-4000-8000-00000000l009',1,1,
                            'e71c5b0d9a1f4c27b3e85a60d4c9f118aa27b4e3',
                            '4a2f77c1b9e0d3568af41c2e7b905dd6134ecb82',
                            'applies',NULL,'[]','2026-01-04T10:00:00Z')",
                    [id.as_str()],
                )
                .expect("freshness row");
        }
        let request =
            format!("GET /api/changes/{id}/applicability HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let response = exchange(
            test_state(temp.path(), MockEngine::available()),
            request.as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        assert_eq!(
            body["schema_version"], 2,
            "the document must declare the version whose shape it serves: {response}"
        );
        assert!(
            body["items"][0].get("checked_paths").is_some(),
            "version 2 is named for `checked_paths`; an item without it is not version 2: \
             {response}"
        );
    }

    /// T-F-1-F1 — the served applicability item names WHAT THE RULE COMPARED
    /// and the version the evidence's own snapshot recorded.
    ///
    /// The "still applies" sentence is "{path} unchanged since {version}", and
    /// before this the item carried neither: `reason_paths` is empty on an
    /// `Applies` record by design (there is nothing invalidating to name) and
    /// there was no version field at all, so the screen rendered "Still
    /// applies — this path unchanged since ." with two holes in it.
    #[test]
    fn the_applicability_route_serves_the_paths_compared_and_the_version_compared_against() {
        let temp = fixture_dir();
        let id = a_store_with_coverage(temp.path());
        {
            let ledger =
                thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
            ledger
                .conn()
                .execute(
                    "INSERT INTO change_criterion_evidence(id, change_id, scope_version,
                         criterion_index, evidence_kind, evidence_id, session_id, basis,
                         detail, actor, asserted_at)
                     VALUES('cccccccc-0000-4000-8000-00000000l001',?1,1,1,'check_run','run1',
                            'cccccccc-0000-4000-8000-00000000s001','declared','clippy',NULL,
                            '2026-01-04T09:00:00Z')",
                    [id.as_str()],
                )
                .expect("link");
            ledger
                .conn()
                .execute(
                    "INSERT INTO change_criterion_freshness(id, change_id, criterion_evidence_id,
                         criterion_index, scope_version, old_snapshot, new_snapshot, state,
                         reason_class, reason_paths_json, recorded_at)
                     VALUES('cccccccc-0000-4000-8000-00000000f001',?1,
                            'cccccccc-0000-4000-8000-00000000l001',1,1,
                            'e71c5b0d9a1f4c27b3e85a60d4c9f118aa27b4e3',
                            '4a2f77c1b9e0d3568af41c2e7b905dd6134ecb82',
                            'applies',NULL,'[]','2026-01-04T10:00:00Z')",
                    [id.as_str()],
                )
                .expect("freshness row");
        }
        let request =
            format!("GET /api/changes/{id}/applicability HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let response = exchange(
            test_state(temp.path(), MockEngine::available()),
            request.as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        let item = &body["items"][0];
        assert_eq!(
            item["checked_paths"],
            serde_json::json!(["crates/**"]),
            "the item must name the scope the rule compared, which the fixture accepted at \
             version 1: {response}"
        );
        assert_eq!(
            item["reason_paths"],
            serde_json::json!([]),
            "an `applies` record names nothing invalidating, which is exactly why the \
             headline cannot be built from it"
        );
        assert_eq!(
            item["old_snapshot"], "e71c5b0d9a1f4c27b3e85a60d4c9f118aa27b4e3",
            "the version the evidence's own snapshot recorded is served"
        );
    }

    #[test]
    fn the_applicability_route_answers_404_for_a_change_that_does_not_exist() {
        let temp = fixture_dir();
        a_store_with_coverage(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/nope/applicability HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n")
                .as_bytes(),
        );
        assert_eq!(status(&response), 404, "{response}");
    }

    /// Every regular file under the folder, added up a second way: a walk
    /// written here, in the test, that shares no line with the one under test.
    fn every_byte_under(dir: &Path) -> u64 {
        let mut total = 0u64;
        let mut stack = vec![dir.to_path_buf()];
        while let Some(next) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&next) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(meta) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                if meta.is_dir() {
                    stack.push(path);
                } else if meta.is_file() {
                    total += meta.len();
                }
            }
        }
        total
    }

    /// `L1.1` / T-2-10 finding 1: the number is THE FOLDER'S number.
    ///
    /// The screen names the folder ("Where the records are kept"), so the
    /// count has to be the folder's — the ledger, the command log, the
    /// preserved transcripts, all of it. The first version of this route
    /// counted the ledger and its two sidecars and was short by more than half
    /// on the store the exit check measured, and the missing half was the
    /// preserved transcripts: the one thing rule 13 exists to warn about.
    ///
    /// Asserted to the byte against a walk written here, so a route that
    /// served a constant, a sampled figure, or one file's length would fail;
    /// and asserted to MOVE when a file appears under `mirrors/`, so a route
    /// that quietly went back to measuring the ledger would fail too.
    #[test]
    fn the_storage_route_counts_every_file_in_the_folder_it_names() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        // A store as a machine in use has one: a command log beside the
        // ledger, and preserved transcripts under a folder of their own.
        std::fs::write(temp.path().join("exec.db"), vec![b'x'; 4_096]).expect("exec store");
        std::fs::create_dir_all(temp.path().join("mirrors/project-a")).expect("mirrors");
        std::fs::write(
            temp.path().join("mirrors/project-a/one.jsonl"),
            vec![b'm'; 10_000],
        )
        .expect("mirror");

        let read = |state: std::sync::Arc<State>| {
            let response = exchange(
                state,
                format!("GET /api/storage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            );
            assert_eq!(status(&response), 200, "{response}");
            response_json(&response)
        };

        let body = read(test_state(temp.path(), MockEngine::available()));
        assert_eq!(body["schema_version"], 4);
        assert_eq!(body["path"], temp.path().display().to_string());
        assert!(body["also"].is_null(), "{body}");
        assert!(body["also_command_log"].is_null(), "{body}");

        let on_disk = every_byte_under(temp.path());
        assert!(on_disk > 14_000, "the fixture store has too little in it");
        assert_eq!(body["bytes"].as_u64(), Some(on_disk), "{body}");

        // The breakdown takes the same number apart and adds back up to it.
        let parts = body["parts"].as_array().cloned().unwrap_or_default();
        let summed: u64 = parts.iter().filter_map(|p| p["bytes"].as_u64()).sum();
        assert_eq!(summed, on_disk, "{body}");
        let part = |key: &str| -> u64 {
            parts
                .iter()
                .find(|p| p["key"] == key)
                .and_then(|p| p["bytes"].as_u64())
                .unwrap_or(0)
        };
        assert_eq!(part("preserved_transcripts"), 10_000, "{body}");
        assert_eq!(part("command_log"), 4_096, "{body}");
        assert!(part("records") > 0, "{body}");

        // One more preserved transcript, and the number a person reads moves
        // by exactly that file — the whole point of measuring the folder.
        std::fs::write(
            temp.path().join("mirrors/project-a/two.jsonl"),
            vec![b'm'; 2_048],
        )
        .expect("second mirror");
        let after = read(test_state(temp.path(), MockEngine::available()));
        assert_eq!(after["bytes"].as_u64(), Some(on_disk + 2_048), "{after}");
        let grown = after["parts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .find(|p| p["key"] == "preserved_transcripts")
            .and_then(|p| p["bytes"].as_u64());
        assert_eq!(grown, Some(12_048), "{after}");
    }

    /// T-7-1 rehearsal: with `THRULINE_HOME` set apart from `HOME`, the command
    /// log lives in a second folder and the served figure left it out —
    /// 1,094,002 bytes said against 1,278,322 kept. The second folder is now
    /// counted, named in `also`, and its `exec.db` is in the breakdown.
    #[test]
    fn the_storage_route_counts_the_execution_store_kept_in_a_folder_of_its_own() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let apart = tempfile::tempdir().expect("THRULINE_HOME");
        std::fs::write(apart.path().join("exec.db"), vec![b'x'; 4_096]).expect("exec store");
        std::fs::create_dir_all(apart.path().join("shim")).expect("shim");
        std::fs::write(apart.path().join("shim/pytest"), vec![b's'; 300]).expect("wrapper");

        let read = |exec_home: Option<PathBuf>| {
            let base = test_state(temp.path(), MockEngine::available());
            let state = Arc::try_unwrap(base).unwrap_or_else(|_| panic!("shared"));
            let response = exchange(
                Arc::new(State { exec_home, ..state }),
                format!("GET /api/storage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            );
            assert_eq!(status(&response), 200, "{response}");
            response_json(&response)
        };

        let here = every_byte_under(temp.path());
        let there = every_byte_under(apart.path());
        assert_eq!(
            there, 4_396,
            "the second folder is not what this test wrote"
        );

        let body = read(Some(apart.path().to_path_buf()));
        assert_eq!(body["bytes"].as_u64(), Some(here + there), "{body}");
        assert_eq!(body["also"], apart.path().display().to_string(), "{body}");
        let parts = body["parts"].as_array().cloned().unwrap_or_default();
        let summed: u64 = parts.iter().filter_map(|p| p["bytes"].as_u64()).sum();
        assert_eq!(
            summed,
            here + there,
            "the breakdown does not add up: {body}"
        );
        let command_log = parts
            .iter()
            .find(|p| p["key"] == "command_log")
            .and_then(|p| p["bytes"].as_u64());
        assert_eq!(command_log, Some(4_096), "{body}");

        // The same folder, or one that holds this one, is not a second folder:
        // counted once, and a `THRULINE_HOME` of `~` never walks a home
        // directory.
        for same in [temp.path().to_path_buf(), temp.path().join("..")] {
            let body = read(Some(same.clone()));
            assert_eq!(body["bytes"].as_u64(), Some(here), "{same:?}: {body}");
            assert!(body["also"].is_null(), "{same:?}: {body}");
        }
        // And unset is the old answer exactly.
        let body = read(None);
        assert_eq!(body["bytes"].as_u64(), Some(here), "{body}");
    }

    /// T-7-1 verifier: with `THRULINE_HOME` equal to `HOME` — a folder that
    /// HOLDS the records folder — the command log is `~/exec.db`, and the
    /// served figure left it out: 1,094,002 bytes said against 1,278,322 kept.
    /// That folder is somebody's home directory and is still never walked; the
    /// execution store this product put there is counted, file by file, and
    /// named in `also_command_log`. Nothing else in it is counted.
    #[test]
    fn the_storage_route_counts_the_command_log_kept_in_a_folder_that_holds_the_records_folder() {
        let home = tempfile::tempdir().expect("HOME");
        let records = home.path().join(".thruline");
        std::fs::create_dir_all(records.join("projects")).expect("records folder");
        std::fs::write(
            records.join(format!("projects/{PROJECT}.json")),
            representative(),
        )
        .expect("fixture");
        a_store_with_one_change(&records);
        // What `THRULINE_HOME=$HOME` puts in the home directory: the execution
        // store and its journal.
        std::fs::write(home.path().join("exec.db"), vec![b'x'; 4_096]).expect("exec store");
        std::fs::write(home.path().join("exec.db-wal"), vec![b'w'; 100]).expect("journal");
        // And what a home directory holds that is not this product's.
        std::fs::write(home.path().join("notes.txt"), vec![b'n'; 10_000]).expect("theirs");
        std::fs::create_dir_all(home.path().join("Documents")).expect("theirs");
        std::fs::write(home.path().join("Documents/a.bin"), vec![b'd'; 5_000]).expect("theirs");

        let read = |exec_home: Option<PathBuf>| {
            let base = test_state(&records, MockEngine::available());
            let state = Arc::try_unwrap(base).unwrap_or_else(|_| panic!("shared"));
            let response = exchange(
                Arc::new(State { exec_home, ..state }),
                format!("GET /api/storage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            );
            assert_eq!(status(&response), 200, "{response}");
            response_json(&response)
        };

        let here = every_byte_under(&records);
        let body = read(Some(home.path().to_path_buf()));
        assert_eq!(body["bytes"].as_u64(), Some(here + 4_196), "{body}");
        assert_eq!(body["schema_version"], 4, "{body}");
        assert!(
            body["also"].is_null(),
            "the home directory is not a second folder: {body}"
        );
        assert_eq!(
            body["also_command_log"],
            home.path().join("exec.db").display().to_string(),
            "{body}"
        );
        let parts = body["parts"].as_array().cloned().unwrap_or_default();
        let summed: u64 = parts.iter().filter_map(|p| p["bytes"].as_u64()).sum();
        assert_eq!(
            summed,
            here + 4_196,
            "the breakdown does not add up: {body}"
        );
        let command_log = parts
            .iter()
            .find(|p| p["key"] == "command_log")
            .and_then(|p| p["bytes"].as_u64());
        assert_eq!(command_log, Some(4_196), "{body}");

        // No execution store there: nothing to add, and nothing named.
        let empty = tempfile::tempdir().expect("a home with no command log");
        let moved = empty.path().join(".thruline");
        std::fs::create_dir_all(&moved).expect("records folder");
        let base = test_state(&moved, MockEngine::available());
        let state = Arc::try_unwrap(base).unwrap_or_else(|_| panic!("shared"));
        let response = exchange(
            Arc::new(State {
                exec_home: Some(empty.path().to_path_buf()),
                ..state
            }),
            format!("GET /api/storage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        let body = response_json(&response);
        assert!(body["also_command_log"].is_null(), "{body}");
        assert_eq!(
            body["bytes"].as_u64(),
            Some(every_byte_under(&moved)),
            "{body}"
        );
    }

    /// It measures a store; it never reads one. A route that served a row of
    /// recorded text would be a second, unmasked door onto the ledger.
    #[test]
    fn the_storage_route_serves_no_recorded_text() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/storage HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        assert!(
            !response.contains("AKIAIOSFODNN7EXAMPLE"),
            "stored text reached the socket: {response}"
        );
        assert!(
            !response.contains("Retry failed uploads"),
            "stored text reached the socket: {response}"
        );
    }

    /// A parameter that did nothing is a caller believing it asked something —
    /// the rule every other read route on this server already holds.
    #[test]
    fn a_query_parameter_the_storage_route_does_not_understand_is_refused() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/storage?project=x HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 400, "{response}");
    }

    #[test]
    fn an_unknown_change_is_a_404_rather_than_an_empty_one() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/nope HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 404, "{response}");
    }

    #[test]
    fn a_query_parameter_the_changes_route_does_not_understand_is_refused() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes?limit=5 HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 400, "{response}");
    }

    /// The Change record itself is still read-only. T-2-2 adds one write
    /// route, and it is not this one: a Change cannot be edited through the
    /// collection.
    #[test]
    fn the_changes_route_accepts_no_writes() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(state, &framed("POST", "/api/changes", b"{}", ""));
        assert_eq!(status(&response), 405, "{response}");
    }

    // -----------------------------------------------------------------------
    // T-2-2 — one write route, and a session's one new read field.
    // -----------------------------------------------------------------------

    const SESSION_WITH_A_LINK: &str = "cccccccc-0000-4000-8000-00000000s001";

    fn same_origin() -> String {
        format!("Origin: http://{ORIGIN}\r\n")
    }

    /// The decision route appends one row and answers with the word a person
    /// reads for it.
    #[test]
    fn a_person_can_confirm_a_link_through_the_one_write_route() {
        let temp = fixture_dir();
        let id = a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let body = serde_json::json!({
            "session": "cccccccc-0000-4000-8000-00000000s001",
            "decision": "confirm",
            "actor": "sam",
            "reason": "this is the one",
        })
        .to_string();
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/associations"),
                body.as_bytes(),
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 200, "{response}");
        let value = response_json(&response);
        assert_eq!(value["provenance"], "confirmed");
        assert_eq!(value["labels"]["confirmed"]["word"], "confirmed link");
    }

    /// No name, no link — and the refusal carries both registers so the screen
    /// can say what would fix it.
    #[test]
    fn the_write_route_refuses_an_unattributed_decision_with_the_reason() {
        let temp = fixture_dir();
        let id = a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let body = serde_json::json!({
            "session": "cccccccc-0000-4000-8000-00000000s001",
            "decision": "confirm",
        })
        .to_string();
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/associations"),
                body.as_bytes(),
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 400, "{response}");
        let value = response_json(&response);
        assert_eq!(value["error"], "actor_required");
        assert_eq!(
            value["plain"],
            "We cannot confirm a link without a name. Say who is confirming it."
        );
        assert!(value["technical"].is_string());
    }

    /// A decision is an action, and actions require the local origin.
    #[test]
    fn the_write_route_requires_the_local_origin() {
        let temp = fixture_dir();
        let id = a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/associations"),
                b"{}",
                "",
            ),
        );
        assert_eq!(status(&response), 403, "{response}");
    }

    /// A session's read-only view gains one field, and a session that belongs
    /// to nothing answers with the sentence rather than a 404.
    #[test]
    fn a_sessions_view_says_which_changes_it_belongs_to_or_that_it_belongs_to_none() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/sessions/{SESSION_WITH_A_LINK}/changes HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n")
                .as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let value = response_json(&response);
        assert_eq!(value["links"]["changes"][0]["label_slug"], "suggested");
        assert_eq!(
            value["lines"]["link_none"]["plain"],
            "No Change is linked to this session yet."
        );
        assert!(value["links"]["none_slug"].is_null());
    }

    #[test]
    fn a_session_linked_to_nothing_is_not_a_404() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/sessions/nobody/changes HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n")
                .as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let value = response_json(&response);
        assert_eq!(value["links"]["none_slug"], "link_none");
        assert_eq!(
            value["links"]["changes"].as_array().map(|a| a.len()),
            Some(0)
        );
    }

    // -----------------------------------------------------------------------
    // T-2-3 — the proposals, read-only, and the one act that accepts them.
    // -----------------------------------------------------------------------

    /// The T-2-1 store, plus the two things a proposal needs: a request written
    /// at the start of a session, and a person who confirmed that session
    /// belongs here.
    fn a_store_with_something_to_propose(dir: &Path) -> String {
        let id = a_store_with_one_change(dir);
        let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                     raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                 VALUES('cccccccc-ev-0001', 'cccccccc-0000-4000-8000-00000000p001',
                        'cccccccc-0000-4000-8000-00000000s001', 1, '2026-01-02T09:30:00Z',
                        'user_msg',
                        'Two retry requests for the same delivery do not enqueue duplicate work. Make retries reliable.',
                        '/tmp/s.jsonl', 1, 'x', '', '')",
                [],
            )
            .expect("opening request");
        thruline_core::change_link::decide(
            &ledger,
            &id,
            SESSION_WITH_A_LINK,
            thruline_core::change::AssociationProvenance::Confirmed,
            "sam",
            "this is the one",
            "2026-01-02T10:05:00Z",
        )
        .expect("confirm");
        id
    }

    /// The proposals, the reason for each, and the sentence for each thing that
    /// stayed a note — all from the copy table, and all masked.
    #[test]
    fn the_proposals_route_serves_the_proposed_lines_and_the_words_for_them() {
        let temp = fixture_dir();
        let id = a_store_with_something_to_propose(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/{id}/proposals HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n")
                .as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        assert!(
            !response.contains("AKIAIOSFODNN7EXAMPLE"),
            "raw stored text reached the socket: {response}"
        );
        let body = response_json(&response);
        assert_eq!(body["schema_version"], 1);
        assert_eq!(body["proposals"]["masking"]["state"], "ran");
        assert_eq!(
            body["proposals"]["criteria"][0]["text"],
            "Two retry requests for the same delivery do not enqueue duplicate work."
        );
        assert_eq!(
            body["proposals"]["criteria"][0]["reasoning_slug"],
            "proposal_reasoning"
        );
        // The masker reaches the notes too, and it ran before the sentence was
        // cut: the Change's purpose is quoted with the credential shape already
        // gone, not truncated around it.
        assert_eq!(
            body["proposals"]["notes"][0]["text"],
            "Use [masked: aws] for the retry"
        );
        // The wish is kept as a note with the part it was missing named, not
        // dropped and not proposed.
        let notes = body["proposals"]["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .map(|n| n["text"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        assert!(
            notes.contains(&"Make retries reliable".to_string()),
            "{notes:?}"
        );
        assert_eq!(
            body["lines"]["note_no_outcome"]["plain"],
            "Kept as a note, not something to check: it does not say what you would see if it were done."
        );
        // A suggested profile is not evidence, and the sentence travels with it.
        assert_eq!(
            body["proposals"]["profile"]["not_evidence_slug"],
            "profile_is_not_evidence"
        );
        assert!(body["lines"]["profile_is_not_evidence"]["plain"]
            .as_str()
            .is_some_and(|line| line.contains("still needs evidence")));
        // Nothing is accepted yet, and it reads as nothing rather than as agreed.
        assert_eq!(
            body["proposals"]["not_accepted_slug"],
            "accepted_scope_none"
        );
    }

    /* ── T-3-4: stopping a check from the screen ───────────────────────── */

    /// A real child, in a group of its own, that the route will stop.
    #[cfg(unix)]
    fn a_running_check(dir: &Path, change_id: &str) -> (String, std::process::Child, i64) {
        use std::os::unix::process::CommandExt;
        let child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 45 & sleep 45")
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("a shell spawns");
        let pgid = i64::from(child.id());
        let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
        let run_key = "cli:check:2026-09-22T10:00:00Z".to_string();
        thruline_core::run_execution::start(
            &ledger,
            &thruline_core::run_execution::Start {
                run_key: run_key.clone(),
                change_id: Some(change_id.to_string()),
                name: "cargo test".into(),
                pid: Some(pgid),
                process_group: Some(pgid),
                started_at: "2026-09-22T10:00:00Z".into(),
                examined_inputs_id: None,
            },
        )
        .expect("the supervisor row appends");
        (run_key, child, pgid)
    }

    /// The whole of `B12.6` through the door a person uses: the run stops, the
    /// record says who asked, and **something looked** at the process group
    /// afterwards rather than assuming it was clean.
    #[cfg(unix)]
    #[test]
    fn stopping_a_check_from_the_screen_records_who_asked_and_verifies_nothing_survived() {
        let temp = fixture_dir();
        let id = a_store_with_one_change(temp.path());
        let (run, mut child, pgid) = a_running_check(temp.path(), &id);
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/runs/{run}/cancel"),
                br#"{"actor":"sam"}"#,
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        assert_eq!(body["cancelled"], true);
        assert_eq!(body["actor"], "sam");
        assert_eq!(body["cleanup_verified"], true);
        assert_eq!(body["survivors"], 0);
        // The record, read back: cancelled by a named person at a time, and
        // NEVER something that settles anything.
        let ledger =
            thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
        let record = thruline_core::run_execution::latest(&ledger, &run)
            .expect("read")
            .expect("a row");
        assert_eq!(
            record.state,
            thruline_core::run_execution::ExecutionState::Cancelled
        );
        assert_eq!(
            record.reason,
            Some(thruline_core::run_execution::Reason::CancelledByPerson)
        );
        assert_eq!(record.actor.as_deref(), Some("sam"));
        assert!(!record.settles, "a stopped check can never settle anything");
        // The process table, independently: the group is empty, grandchild
        // included.
        assert_eq!(
            crate::run_supervisor::group_survivors(pgid),
            crate::run_supervisor::Cleanup::Verified
        );
        let _ = child.wait();
    }

    /// A second cancellation is refused rather than appended: a run cannot end
    /// twice, and a record that said so would be contradicting itself.
    #[cfg(unix)]
    #[test]
    fn a_check_that_already_stopped_is_refused_rather_than_stopped_again() {
        let temp = fixture_dir();
        let id = a_store_with_one_change(temp.path());
        let (run, mut child, _) = a_running_check(temp.path(), &id);
        for expected in [200, 409] {
            let state = test_state(temp.path(), MockEngine::available());
            let response = exchange(
                state,
                &framed(
                    "POST",
                    &format!("/api/changes/{id}/runs/{run}/cancel"),
                    br#"{"actor":"sam"}"#,
                    &same_origin(),
                ),
            );
            assert_eq!(status(&response), expected, "{response}");
        }
        let _ = child.wait();
    }

    /// The route cannot reach across Changes by guessing a run key, and it
    /// refuses an act with no name on it.
    #[cfg(unix)]
    #[test]
    fn the_cancel_route_refuses_another_changes_run_and_an_unnamed_act() {
        let temp = fixture_dir();
        let id = a_store_with_one_change(temp.path());
        let (run, mut child, pgid) = a_running_check(temp.path(), &id);

        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/runs/{run}/cancel"),
                b"{}",
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 400, "an act with no name is refused");

        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/not-this-one/runs/{run}/cancel"),
                br#"{"actor":"sam"}"#,
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 404, "{response}");

        // Neither refusal touched the process, and the run is still going.
        let ledger =
            thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
        assert_eq!(
            thruline_core::run_execution::state_of(&ledger, &run).expect("read"),
            thruline_core::run_execution::ExecutionState::Running
        );
        let _ = crate::run_supervisor::stop_group(pgid, std::time::Duration::from_secs(3));
        let _ = child.wait();
    }

    /// Reading proposals is a read. The route takes no writes at all.
    #[test]
    fn the_proposals_route_takes_no_writes() {
        let temp = fixture_dir();
        let id = a_store_with_something_to_propose(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/proposals"),
                b"{}",
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 405, "{response}");
    }

    #[test]
    fn a_change_that_does_not_exist_has_no_proposals() {
        let temp = fixture_dir();
        a_store_with_something_to_propose(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/changes/nope/proposals HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n")
                .as_bytes(),
        );
        assert_eq!(status(&response), 404, "{response}");
        assert_eq!(response_json(&response)["error"], "no_such_change");
    }

    /// One act, one version, one acceptance record — and no approval.
    #[test]
    fn a_person_can_accept_what_counts_as_done_through_the_one_write_route() {
        let temp = fixture_dir();
        let id = a_store_with_something_to_propose(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let body = serde_json::json!({
            "all": true,
            "actor": "sam",
            "rationale": "this is what I want checked",
        })
        .to_string();
        let response = exchange(
            state.clone(),
            &framed(
                "POST",
                &format!("/api/changes/{id}/scope/accept"),
                body.as_bytes(),
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 200, "{response}");
        let value = response_json(&response);
        assert_eq!(value["accepted"]["version"], 1);
        assert_eq!(value["accepted"]["actor"], "sam");
        assert_eq!(
            value["accepted"]["criteria"][0]["attribution"],
            "as_proposed"
        );
        assert_eq!(
            value["lines"]["accept_is_not_approval"]["plain"],
            "Agreeing what counts as done is not approving the work. Nobody has approved anything."
        );

        // And the Change itself still has no decision: accepting is not
        // approving, read back from the record rather than promised.
        let response = exchange(
            state,
            format!("GET /api/changes/{id} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        let after = response_json(&response);
        assert_eq!(after["change"]["decision"], "none_yet");
        assert_eq!(after["change"]["accepted_scope"]["version"], 1);
        assert_eq!(
            after["change"]["accepted_scope"]["acceptance"]["actor"],
            "sam"
        );
    }

    /// No name, no acceptance — and the refusal carries both registers.
    #[test]
    fn the_accept_route_refuses_an_unattributed_acceptance_with_the_reason() {
        let temp = fixture_dir();
        let id = a_store_with_something_to_propose(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let body = serde_json::json!({"all": true}).to_string();
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/scope/accept"),
                body.as_bytes(),
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 400, "{response}");
        let value = response_json(&response);
        assert_eq!(value["error"], "actor_required");
        assert_eq!(
            value["plain"],
            "We cannot write down what counts as done without a name. Say who is accepting it."
        );
        assert!(value["technical"].is_string());
    }

    /// A body that says neither "all" nor which lines is refused rather than
    /// guessed at.
    #[test]
    fn the_accept_route_refuses_a_body_that_does_not_say_which_lines() {
        let temp = fixture_dir();
        let id = a_store_with_something_to_propose(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let body = serde_json::json!({"actor": "sam"}).to_string();
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/scope/accept"),
                body.as_bytes(),
                &same_origin(),
            ),
        );
        assert_eq!(status(&response), 400, "{response}");
        assert_eq!(response_json(&response)["error"], "say_all_or_which");
    }

    /// Accepting is an action, and actions require the local origin.
    #[test]
    fn the_accept_route_requires_the_local_origin() {
        let temp = fixture_dir();
        let id = a_store_with_something_to_propose(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            &framed(
                "POST",
                &format!("/api/changes/{id}/scope/accept"),
                b"{}",
                "",
            ),
        );
        assert_eq!(status(&response), 403, "{response}");
    }

    /// `/api/sessions` itself is untouched: no parameter, same answer.
    #[test]
    fn the_session_list_route_is_unchanged_by_the_change_layer() {
        let temp = fixture_dir();
        a_store_with_one_change(temp.path());
        let state = test_state(temp.path(), MockEngine::available());
        let response = exchange(
            state,
            format!("GET /api/sessions?limit=5 HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(
            status(&response),
            400,
            "the unknown-parameter refusal is the one T-2-1 left: {response}"
        );
    }

    #[test]
    fn exit_three_is_a_policy_answer_and_is_served_as_one() {
        // §14: 0 ok, 1 usage/error, 3 policy failure. An unsupported claim is
        // exactly what a reader of `/api/explain` came to see, so 3 must reach
        // them as an ANSWER — 200, with the code in a header — while a real
        // failure must not be dressed up as one.
        let temp = fixture_dir();
        for (code, expected_status) in [(0, 200), (3, 200), (1, 502), (101, 502)] {
            let mut state = test_state(temp.path(), MockEngine::available());
            let state_mut = Arc::get_mut(&mut state).expect("sole owner");
            state_mut.exec_home = Some(temp.path().to_path_buf());
            state_mut.cli_exe = fake_cli(
                temp.path(),
                &format!("cli{code}"),
                r#"{"session":"s"}"#,
                code,
            );
            let response = exchange(
                state,
                format!("GET /api/explain HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            );
            assert_eq!(
                status(&response),
                expected_status,
                "exit {code} must serve {expected_status}; got: {response}"
            );
            assert!(
                response.contains(&format!("X-ThruLine-Exit: {code}")),
                "exit {code} must be reported in the header: {response}"
            );
            if expected_status == 200 {
                assert_eq!(
                    response_json(&response),
                    serde_json::json!({"session": "s"}),
                    "the CLI's body must arrive unwrapped"
                );
            } else {
                assert_eq!(
                    response_json(&response)["error"],
                    serde_json::json!("projection_failed")
                );
            }
        }
    }

    #[test]
    fn a_projected_session_without_a_policy_root_refuses_path_bearing_text() {
        let temp = fixture_dir();
        let db = temp.path().join("exec.db");
        let exec = thruline_ledger::Ledger::open(&db).expect("synthetic execution store");
        let repo = exec
            .repo("unresolved-project", None)
            .expect("synthetic repo");
        exec.upsert_session("s", &repo, "claude-code", None, false, None)
            .expect("synthetic session");
        drop(exec);

        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.exec_home = Some(temp.path().to_path_buf());
        state_mut.cli_exe = fake_cli(
            temp.path(),
            "path-cli",
            r#"{"session":"s","what_happened":[{"text":"Read src/a.rs"}]}"#,
            0,
        );
        let response = exchange(
            state,
            format!("GET /api/explain HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 503, "{response}");
        assert!(response.contains("X-ThruLine-Projection: explain --json"));
        assert!(response.contains("X-ThruLine-Exit: 0"));
        assert_eq!(
            response_json(&response)["error"],
            "exclusion_policy_unavailable"
        );
        assert!(!response.contains("src/a.rs"), "{response}");
    }

    #[test]
    fn an_execution_session_uses_its_own_exclusion_when_capture_id_is_reused() {
        let temp = fixture_dir();
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("capture ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id,project_id,file_path) VALUES('s',?1,'/synthetic/s.jsonl')",
                [PROJECT],
            )
            .expect("capture session with reused id");
        let hold = thruline_core::capture_hold::Hold {
            project_id: "other-project",
            project_folder: "/workspace/other-project",
            actor: "synthetic person",
            recorded_at: "2026-08-25T00:00:01Z",
        };
        thruline_core::capture_hold::record_for_person(
            &ledger,
            thruline_core::capture_hold::Act::Excluded,
            &hold,
            Some("src/module-00.rs"),
        )
        .expect("execution project's exclusion");
        drop(ledger);

        let exec = thruline_ledger::Ledger::open(&temp.path().join("exec.db"))
            .expect("synthetic execution store");
        let repo = exec
            .repo("/workspace/other-project", Some("/workspace/other-project"))
            .expect("execution project");
        exec.upsert_session("s", &repo, "claude-code", None, false, None)
            .expect("execution session with reused id");
        drop(exec);

        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.exec_home = Some(temp.path().to_path_buf());
        state_mut.cli_exe = fake_cli(
            temp.path(),
            "collision-cli",
            r#"{"session":"s","what_happened":[{"text":"Read src/module-00.rs","output":"private output"}]}"#,
            0,
        );
        let response = exchange(
            state,
            format!("GET /api/explain?session=s HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        assert_eq!(response_json(&response)[LEFT_OUT_FIELD], 1);
        assert_eq!(
            response_json(&response)["what_happened"][0]["text"],
            "left out by your exclusion (`src/module-00.rs`)"
        );
        assert!(!response.contains("private output"), "{response}");
    }

    #[test]
    fn a_verification_repo_wins_when_capture_reuses_an_unknown_execution_session() {
        let temp = fixture_dir();
        let verification_repo = temp.path().join("verification-project");
        std::fs::create_dir(&verification_repo).expect("verification repository");
        let verification_root = verification_repo.to_str().expect("UTF-8 scratch path");
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("capture ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id,project_id,file_path) VALUES('s',?1,'/synthetic/s.jsonl')",
                [PROJECT],
            )
            .expect("capture session with reused id");
        let hold = thruline_core::capture_hold::Hold {
            project_id: "verification-project",
            project_folder: verification_root,
            actor: "synthetic person",
            recorded_at: "2026-08-25T00:00:01Z",
        };
        thruline_core::capture_hold::record_for_person(
            &ledger,
            thruline_core::capture_hold::Act::Excluded,
            &hold,
            Some("src/module-00.rs"),
        )
        .expect("verification project's exclusion");
        drop(ledger);
        let exec_db = temp.path().join("exec.db");
        drop(thruline_ledger::Ledger::open(&exec_db).expect("empty execution store"));

        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.verification = Some(LocalVerification {
            repo: verification_repo,
            db: exec_db,
            job: Mutex::new(LocalJob::default()),
        });
        state_mut.cli_exe = fake_cli(
            temp.path(),
            "verification-collision-cli",
            r#"{"session":"s","what_happened":[{"text":"Read src/module-00.rs","output":"private output"}]}"#,
            0,
        );
        let response = exchange(
            state,
            format!("GET /api/explain?session=s HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        assert_eq!(response_json(&response)[LEFT_OUT_FIELD], 1);
        assert_eq!(
            response_json(&response)["what_happened"][0]["text"],
            "left out by your exclusion (`src/module-00.rs`)"
        );
        assert!(!response.contains("private output"), "{response}");
    }

    #[test]
    fn a_failed_projection_filters_its_stderr_without_changing_exit_metadata() {
        let temp = fixture_dir();
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("capture ledger");
        fixture_exclusion(&ledger, thruline_core::capture_hold::Act::Excluded);
        drop(ledger);

        let exec = thruline_ledger::Ledger::open(&temp.path().join("exec.db"))
            .expect("synthetic execution store");
        let repo = exec
            .repo(
                "/workspace/thruline-synthetic",
                Some("/workspace/thruline-synthetic"),
            )
            .expect("synthetic repo");
        exec.upsert_session("s", &repo, "claude-code", None, false, None)
            .expect("synthetic session");
        drop(exec);

        let executable = fake_cli(temp.path(), "failed-cli", "", 1);
        let diagnostic = format!("{} src/module-00.rs", "x".repeat(2_010));
        std::fs::write(
            &executable,
            format!("#!/bin/sh\nprintf '%s' '{diagnostic}' >&2\nexit 1\n"),
        )
        .expect("fake stderr");
        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.exec_home = Some(temp.path().to_path_buf());
        state_mut.cli_exe = executable.clone();
        let request = format!("GET /api/explain?session=s HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let response = exchange(state, request.as_bytes());
        assert_eq!(status(&response), 502, "{response}");
        assert!(response.contains("X-ThruLine-Projection: explain --json"));
        assert!(response.contains("X-ThruLine-Exit: 1"));
        let body = response_json(&response);
        assert_eq!(body["error"], "projection_failed");
        assert_eq!(body["left_out_by_exclusion"], 1);
        assert_eq!(
            body["stderr"],
            "left out by your exclusion (`src/module-00.rs`)"
        );

        // A successful phase stream has the same cut boundary for unparsed
        // stderr. Match its full line before presenting the bounded version.
        std::fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s' '{{\"session\":\"s\"}}'\nprintf '%s' '{diagnostic}' >&2\nexit 0\n"
            ),
        )
        .expect("fake phase stderr");
        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.exec_home = Some(temp.path().to_path_buf());
        state_mut.cli_exe = executable;
        let response = exchange(
            state,
            format!("GET /api/phases?session=s HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        assert_eq!(body["left_out_by_exclusion"], 1);
        assert_eq!(
            body["unparsed"][0],
            "left out by your exclusion (`src/module-00.rs`)"
        );
    }

    /// `/api/sessions` is the same projection mechanism as `/api/explain`, and
    /// this witnesses it without a real store: the same 0/3 → 200, the same
    /// everything-else → 502, the same two headers, the same unwrapped body.
    #[test]
    fn the_session_list_is_projected_under_the_same_exit_rule() {
        let temp = fixture_dir();
        for (code, expected_status) in [(0, 200), (3, 200), (1, 502), (101, 502)] {
            let mut state = test_state(temp.path(), MockEngine::available());
            Arc::get_mut(&mut state).expect("sole owner").cli_exe = fake_cli(
                temp.path(),
                &format!("sessions-cli{code}"),
                r#"{"total":0,"sessions":[]}"#,
                code,
            );
            let response = exchange(
                state,
                format!("GET /api/sessions HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            );
            assert_eq!(
                status(&response),
                expected_status,
                "exit {code} must serve {expected_status}; got: {response}"
            );
            assert!(
                response.contains("X-ThruLine-Projection: sessions --json"),
                "the projection must name the command it is the output of: {response}"
            );
            assert!(
                response.contains(&format!("X-ThruLine-Exit: {code}")),
                "exit {code} must be reported in the header: {response}"
            );
            if expected_status == 200 {
                assert_eq!(
                    response_json(&response),
                    serde_json::json!({"total": 0, "sessions": []}),
                    "the CLI's body must arrive unwrapped"
                );
            } else {
                assert_eq!(
                    response_json(&response)["error"],
                    serde_json::json!("projection_failed")
                );
                assert_eq!(
                    response_json(&response)["command"],
                    serde_json::json!("thruline sessions --json")
                );
            }
        }
    }

    /// R-2026-09-24-27 — a handler slower than one read poll still gets its
    /// answer delivered, whole.
    ///
    /// This is the defect three lanes called contention. `/api/explain`,
    /// `/api/sessions` and `/api/phases` cannot produce a first byte until a
    /// child process has been spawned, run and reaped, and under load that took
    /// over the five seconds the old `exchange` allowed — so the client read
    /// returned `EAGAIN` and the test died with a bare `WouldBlock` while the
    /// handler was still working (measured: `bytes = 0`, handler thread still
    /// running, every time). Nothing here needs a loaded machine to witness it:
    /// a child that sleeps past the poll interval reproduces exactly the same
    /// read, deterministically, and the response must still arrive intact.
    #[test]
    #[cfg(unix)]
    fn a_handler_slower_than_one_read_poll_is_waited_for_not_abandoned() {
        let slow = Duration::from_secs(1);
        assert!(
            slow > READ_POLL * 2,
            "this test must outlast a read poll to witness anything: \
             {slow:?} against {READ_POLL:?}"
        );
        let temp = fixture_dir();
        let script = temp.path().join("slow-cli");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nsleep {}\nprintf '%s' '{{\"session\":\"s\"}}'\n",
                slow.as_secs()
            ),
        )
        .expect("script");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let mut state = test_state(temp.path(), MockEngine::available());
        Arc::get_mut(&mut state).expect("sole owner").cli_exe = script;
        let started = std::time::Instant::now();
        let response = exchange(
            state,
            format!("GET /api/explain HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert!(
            started.elapsed() >= slow,
            "the harness must have waited for the child, not answered from somewhere else"
        );
        assert_eq!(status(&response), 200, "{response}");
        assert_eq!(
            response_json(&response),
            serde_json::json!({"session": "s"}),
            "the whole body must survive a wait that spans several read polls"
        );
    }

    /// A parameter that does nothing must be refused, not ignored — and the
    /// refusal must happen before a child is spawned. The sentinel is the proof
    /// of the second half: an ignored `?limit=` would still have run the CLI.
    #[test]
    fn the_session_list_takes_no_query_parameters_and_spawns_nothing() {
        let temp = fixture_dir();
        let sentinel = temp.path().join("spawned");
        let script = temp.path().join("sentinel-cli");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ntouch '{}'\nprintf '%s' '{{}}'\n",
                sentinel.display()
            ),
        )
        .expect("script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        // A bare `?` carries no parameter and is not in this list: it is the
        // empty query, which is what "no parameters" means.
        for query in ["?session=abc", "?limit=5", "?limit=5&session=abc"] {
            let mut state = test_state(temp.path(), MockEngine::available());
            Arc::get_mut(&mut state).expect("sole owner").cli_exe = script.clone();
            let response = exchange(
                state,
                format!("GET /api/sessions{query} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            );
            assert_eq!(
                status(&response),
                400,
                "{query} must be refused: {response}"
            );
            assert_eq!(
                response_json(&response)["error"],
                serde_json::json!("unknown_query_parameter")
            );
            assert!(
                !sentinel.exists(),
                "{query} was refused only after the CLI had already run"
            );
        }
    }

    /// Unknown stays unknown across the server, not only in the CLI. The engine
    /// writes `elapsed_ms: null` for a phase it could not time (§2.1.1), and
    /// `/api/phases` must serve that row exactly as written: a server that
    /// "repairs" the null into a number — a sentinel or a 0 — would hand the
    /// panel a count it never measured. No real session can produce the null
    /// (it needs a duration too long to count in a 64-bit number of
    /// milliseconds), so the stand-in CLI writes it to stderr the way the
    /// engine does.
    #[test]
    fn a_phase_the_engine_could_not_time_is_served_as_null_never_a_number() {
        let temp = fixture_dir();
        let exec = thruline_ledger::Ledger::open(&temp.path().join("exec.db"))
            .expect("synthetic execution store");
        let repo = exec
            .repo(
                "/workspace/thruline-synthetic",
                Some("/workspace/thruline-synthetic"),
            )
            .expect("synthetic repo");
        exec.upsert_session("s1", &repo, "claude-code", None, false, None)
            .expect("synthetic session");
        drop(exec);
        let untimed = serde_json::json!({
            "phase": "read_record", "count": 3, "detail": "session s1", "elapsed_ms": null
        });
        let timed = serde_json::json!({
            "phase": "find_claims", "count": 0, "detail": "no completion claim in this session", "elapsed_ms": 5
        });
        let script = temp.path().join("phases-cli");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' '{untimed}' >&2\nprintf '%s\\n' '{timed}' >&2\nprintf '%s' '{{\"session\":\"s1\"}}'\n"
            ),
        )
        .expect("script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        let mut state = test_state(temp.path(), MockEngine::available());
        let state_mut = Arc::get_mut(&mut state).expect("sole owner");
        state_mut.exec_home = Some(temp.path().to_path_buf());
        state_mut.cli_exe = script;
        let response = exchange(
            state,
            format!("GET /api/phases?session=s1 HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
        );
        assert_eq!(status(&response), 200, "{response}");
        let body = response_json(&response);
        assert_eq!(
            body["phases"],
            serde_json::json!([untimed, timed]),
            "the phase rows must be served as the engine wrote them"
        );
        assert!(
            body["phases"][0]["elapsed_ms"].is_null(),
            "an untimed phase must reach the panel as null: {body}"
        );
        assert!(body.get("unparsed").is_none(), "{body}");
    }

    #[test]
    fn only_loopback_addresses_can_be_bound() {
        assert_eq!(resolve_bind_address(None).unwrap(), Ipv4Addr::LOCALHOST);
        assert_eq!(
            resolve_bind_address(Some("localhost")).unwrap(),
            Ipv4Addr::LOCALHOST
        );
        assert_eq!(
            resolve_bind_address(Some("127.0.0.1")).unwrap(),
            Ipv4Addr::LOCALHOST
        );
        assert!(resolve_bind_address(Some("127.0.0.53")).is_ok());
        for refused in ["0.0.0.0", "::", "::1", "192.168.1.10", "example.com", ""] {
            let err = resolve_bind_address(Some(refused))
                .expect_err("must refuse {refused}")
                .to_string();
            assert!(err.contains(refused) || refused.is_empty(), "{err}");
            assert!(err.contains("loopback"), "{err}");
        }
    }

    #[test]
    fn the_session_parameter_is_an_identifier_or_a_refusal() {
        assert_eq!(session_param(""), Ok(None));
        assert_eq!(
            session_param("session=abc-123.4_5"),
            Ok(Some("abc-123.4_5".to_string()))
        );
        // Percent-encoding does not smuggle anything past the check.
        assert_eq!(session_param("session=a%20b"), Err("invalid_session"));
        assert_eq!(
            session_param("session=%2D%2Dexec%2Ddb"),
            Err("invalid_session")
        );
        for refused in [
            "session=--exec-db",
            "session=../../etc/passwd",
            "session=.",
            "session=",
            "session=a&session=b",
            "other=1",
        ] {
            assert!(session_param(refused).is_err(), "{refused} must be refused");
        }
    }

    #[test]
    fn a_static_root_must_be_a_directory_with_an_index() {
        let temp = tempfile::tempdir().expect("temp");
        assert_eq!(resolve_static_root(None).unwrap(), None);

        let missing = temp.path().join("nope");
        let err = resolve_static_root(Some(&missing)).unwrap_err().to_string();
        assert!(err.contains("no static directory at"), "{err}");
        assert!(err.contains(&missing.display().to_string()), "{err}");

        let file = temp.path().join("file");
        std::fs::write(&file, b"x").expect("file");
        assert!(resolve_static_root(Some(&file))
            .unwrap_err()
            .to_string()
            .contains("not a directory"));

        let dir = temp.path().join("dist");
        std::fs::create_dir_all(&dir).expect("dir");
        assert!(resolve_static_root(Some(&dir))
            .unwrap_err()
            .to_string()
            .contains("no index.html"));

        std::fs::write(dir.join("index.html"), b"<!doctype html>").expect("index");
        assert_eq!(
            resolve_static_root(Some(&dir)).unwrap(),
            Some(dir.canonicalize().expect("canonical"))
        );
    }

    #[test]
    fn fixture_paths_cannot_climb_out() {
        assert!(safe_fixture_path("index.json").is_some());
        assert!(safe_fixture_path("projects/abc-123.json").is_some());
        assert!(safe_fixture_path("../../.thruline/ledger.db").is_none());
        assert!(safe_fixture_path("projects/../../secrets.json").is_none());
        assert!(safe_fixture_path("projects/nested/deep.json").is_none());
        assert!(safe_fixture_path("ledger.db").is_none());
        assert!(safe_fixture_path("").is_none());
    }

    #[test]
    fn a_staged_project_fixture_and_model_input_follow_exclude_then_include() {
        let temp = fixture_dir();
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("fixture ledger");
        let raw = representative();
        let (before, baseline) = present_project_fixture(&ledger, raw).expect("plain fixture");
        assert!(matches!(&before, Cow::Borrowed(_)));
        assert_eq!(before.as_ref(), raw);

        fixture_exclusion(&ledger, thruline_core::capture_hold::Act::Excluded);
        let (hidden, excluded) = present_project_fixture(&ledger, raw).expect("hidden fixture");
        assert_ne!(excluded, baseline);
        assert!(matches!(&hidden, Cow::Owned(_)));
        assert!(
            !String::from_utf8_lossy(hidden.as_ref())
                .contains("/workspace/thruline-synthetic/src/module-00.rs"),
            "the served fixture must not carry the original absolute path"
        );
        assert!(
            String::from_utf8_lossy(hidden.as_ref())
                .contains("left out by your exclusion (`src/module-00.rs`)"),
            "the served fixture must carry the fixed marker"
        );
        let catalog = catalog_from_export(hidden.as_ref(), PROJECT, SESSION).expect("safe catalog");
        let original_catalog = catalog_from_export(raw, PROJECT, SESSION).expect("plain catalog");
        assert_ne!(catalog.source_digest(), original_catalog.source_digest());
        assert!(
            !catalog
                .model_material()
                .contains("Work cluster at src/module-00.rs"),
            "the local model must not receive the original work-cluster path"
        );
        assert!(
            catalog
                .model_material()
                .contains("left out by your exclusion"),
            "the local model receives the marker in place of the original"
        );

        let state = test_state(temp.path(), MockEngine::available());
        let get =
            format!("GET /fixtures/projects/{PROJECT}.json HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let response = exchange(Arc::clone(&state), get.as_bytes());
        assert_eq!(status(&response), 200, "{response}");
        assert!(
            !response.contains("/workspace/thruline-synthetic/src/module-00.rs"),
            "the HTTP route must not serve the original absolute path"
        );

        fixture_exclusion(&ledger, thruline_core::capture_hold::Act::Included);
        let (restored, restored_fingerprint) =
            present_project_fixture(&ledger, raw).expect("restored fixture");
        assert!(matches!(&restored, Cow::Borrowed(_)));
        assert_eq!(restored.as_ref(), raw);
        assert_eq!(restored_fingerprint, baseline);
        let response = exchange(state, get.as_bytes());
        assert_eq!(status(&response), 200, "{response}");
        assert!(response.contains("module-00.rs"));
    }

    #[test]
    fn a_staged_index_filters_only_its_own_project_row_and_restores_it() {
        let temp = fixture_dir();
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("fixture ledger");
        let other = thruline_core::model::ProjectRef {
            id: "other-synthetic-project".into(),
            root_path: "/workspace/other".into(),
            source: thruline_core::model::Source::Codex,
            display_name: Some("Other".into()),
        };
        ledger
            .upsert_project(&other, "2026-08-25T00:00:00Z")
            .expect("other project");
        let raw = serde_json::to_vec(&serde_json::json!({
            "kind": "index",
            "projects": [
                {"fixture": thruline_core::export::project_fixture_id(PROJECT),
                 "attention": {"items": [{"sentence": "Fix src/module-00.rs."}]}},
                {"fixture": thruline_core::export::project_fixture_id(&other.id),
                 "attention": {"items": [{"sentence": "Fix src/module-00.rs."}]}}
            ]
        }))
        .expect("index");
        let before = present_index_fixture(&ledger, &raw).expect("plain index");
        assert!(matches!(&before, Cow::Borrowed(_)));

        fixture_exclusion(&ledger, thruline_core::capture_hold::Act::Excluded);
        let hidden = present_index_fixture(&ledger, &raw).expect("hidden index");
        let doc: serde_json::Value = serde_json::from_slice(hidden.as_ref()).expect("JSON");
        assert_eq!(doc["left_out_by_exclusion"], 1);
        assert_eq!(
            doc["projects"][0]["attention"]["items"][0]["sentence"],
            "left out by your exclusion (`src/module-00.rs`)"
        );
        assert_eq!(
            doc["projects"][1]["attention"]["items"][0]["sentence"],
            "Fix src/module-00.rs."
        );

        fixture_exclusion(&ledger, thruline_core::capture_hold::Act::Included);
        let restored = present_index_fixture(&ledger, &raw).expect("restored index");
        assert!(matches!(&restored, Cow::Borrowed(_)));
        assert_eq!(restored.as_ref(), raw);
    }

    #[test]
    fn a_stale_index_row_is_refused_even_when_the_ledger_has_no_active_policy() {
        let temp = tempfile::tempdir().expect("temporary ledger");
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
        let stale = br#"{"projects":[{"fixture":"0123456789abcdef"}]}"#;
        assert!(
            present_index_fixture(&ledger, stale).is_err(),
            "an orphaned row cannot bypass policy lookup through the empty-policy shortcut"
        );
    }

    #[test]
    fn the_ui_is_actually_embedded() {
        // Guards the failure this whole module exists to prevent: a binary that
        // starts, serves, opens a browser, and has no app inside it.
        assert!(
            assets::UI_ASSETS.iter().any(|(p, _, _)| *p == "index.html"),
            "no index.html embedded"
        );
        assert!(
            assets::UI_ASSETS
                .iter()
                .any(|(p, _, _)| p.starts_with("assets/") && p.ends_with(".js")),
            "no application bundle embedded"
        );
    }

    #[test]
    fn durations_read_like_english() {
        assert_eq!(human_duration(0.42), "420 ms");
        assert_eq!(human_duration(12.0), "12.0 seconds");
        assert_eq!(human_duration(600.0), "10.0 minutes");
    }

    #[test]
    fn mind_status_is_guarded_bounded_and_never_analyzes() {
        let temp = fixture_dir();
        let engine = MockEngine::available();
        let state = test_state(temp.path(), Arc::clone(&engine));
        let request = format!("GET /api/mind/status HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        let response = exchange(Arc::clone(&state), request.as_bytes());
        assert_eq!(status(&response), 200);
        let body = response_json(&response);
        assert_eq!(body["available"], true);
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["model_digest"], MODEL_DIGEST);
        assert_eq!(body["provider"], "ollama");
        assert_eq!(body["mode"], "shadow");
        assert_eq!(body["truth_bearing"], false);
        assert_eq!(engine.probes.load(Ordering::SeqCst), 1);
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 0);

        let bad_host = b"GET /api/mind/status HTTP/1.1\r\nHost: attacker.invalid\r\n\r\n";
        assert_eq!(status(&exchange(Arc::clone(&state), bad_host)), 403);
        let bad_origin = format!(
            "GET /api/mind/status HTTP/1.1\r\nHost: {ORIGIN}\r\nOrigin: https://attacker.invalid\r\n\r\n"
        );
        assert_eq!(
            status(&exchange(Arc::clone(&state), bad_origin.as_bytes())),
            403
        );
        let duplicate_host =
            format!("GET /api/mind/status HTTP/1.1\r\nHost: {ORIGIN}\r\nHost: {ORIGIN}\r\n\r\n");
        assert_eq!(
            status(&exchange(Arc::clone(&state), duplicate_host.as_bytes())),
            400
        );
        let duplicate_origin = format!(
            "GET /api/mind/status HTTP/1.1\r\nHost: {ORIGIN}\r\nOrigin: http://{ORIGIN}\r\nOrigin: http://{ORIGIN}\r\n\r\n"
        );
        assert_eq!(
            status(&exchange(Arc::clone(&state), duplicate_origin.as_bytes())),
            400
        );
        let query =
            format!("GET /api/mind/status?prompt=private HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        assert_eq!(status(&exchange(Arc::clone(&state), query.as_bytes())), 400);
        let wrong_method = format!("POST /api/mind/status HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        assert_eq!(
            status(&exchange(Arc::clone(&state), wrong_method.as_bytes())),
            405
        );
        for entity_headers in [
            "Content-Length: 0\r\n",
            "Content-Length: 0\r\nContent-Length: 0\r\n",
            "Content-Type: application/json\r\n",
            "Transfer-Encoding: chunked\r\n",
        ] {
            let request =
                format!("GET /api/mind/status HTTP/1.1\r\nHost: {ORIGIN}\r\n{entity_headers}\r\n");
            assert_eq!(
                status(&exchange(Arc::clone(&state), request.as_bytes())),
                400
            );
        }
        assert_eq!(engine.probes.load(Ordering::SeqCst), 1);

        engine.available.store(false, Ordering::SeqCst);
        let response = exchange(state, request.as_bytes());
        assert_eq!(status(&response), 200);
        let body = response_json(&response);
        assert_eq!(body["available"], false);
        assert!(body["model_digest"].is_null());
        assert!(!response.contains("PRIVATE_PROVIDER_FAILURE_DETAIL"));
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 0);

        assert!(peer_is_allowed("127.0.0.1:1".parse().expect("loopback")));
        assert!(peer_is_allowed("[::1]:1".parse().expect("v6 loopback")));
        assert!(!peer_is_allowed("192.0.2.1:1".parse().expect("remote")));
    }

    #[test]
    fn interpret_framing_rejects_malformed_oversize_and_caller_text_before_engine() {
        let temp = fixture_dir();
        let engine = MockEngine::available();
        let state = test_state(temp.path(), Arc::clone(&engine));
        let body = valid_body();
        let missing_length = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Type: application/json\r\n\r\n{body}"
        );
        let duplicate_length = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: {}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
            body.len(), body.len()
        );
        let transfer = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: {}\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n{body}",
            body.len()
        );
        let duplicate_type = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nContent-Type: application/json\r\n\r\n{body}",
            body.len()
        );
        let repeated_charset = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: {}\r\nContent-Type: application/json; charset=utf-8; charset=utf-8\r\n\r\n{body}",
            body.len()
        );
        let wrong_type = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{body}",
            body.len()
        );
        let missing_type = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let malformed_header = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nBroken-Header\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
            body.len()
        );
        let non_decimal = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: +{}\r\nContent-Type: application/json\r\n\r\n{body}",
            body.len()
        );
        let zero = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: 0\r\nContent-Type: application/json\r\n\r\n"
        );
        let short = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
            body.len() + 1
        );
        let malformed_json = framed("POST", "/api/mind/interpret", b"{", "");
        let unknown = serde_json::json!({
            "project":PROJECT,"session":SESSION,"model":MODEL,"prompt":"private"
        })
        .to_string();
        let unknown = framed("POST", "/api/mind/interpret", unknown.as_bytes(), "");
        let invalid_utf8 = framed("POST", "/api/mind/interpret", &[0xff, 0xfe], "");
        let mut trailing = framed("POST", "/api/mind/interpret", body.as_bytes(), "");
        trailing.extend_from_slice(b"X");

        for request in [
            missing_length.into_bytes(),
            duplicate_length.into_bytes(),
            transfer.into_bytes(),
            duplicate_type.into_bytes(),
            repeated_charset.into_bytes(),
            wrong_type.into_bytes(),
            missing_type.into_bytes(),
            malformed_header.into_bytes(),
            non_decimal.into_bytes(),
            zero.into_bytes(),
            short.into_bytes(),
            malformed_json,
            unknown,
            invalid_utf8,
            trailing,
        ] {
            assert_eq!(status(&exchange(Arc::clone(&state), &request)), 400);
        }

        let oversize = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nContent-Length: 513\r\nContent-Type: application/json\r\n\r\n"
        );
        assert_eq!(
            status(&exchange(Arc::clone(&state), oversize.as_bytes())),
            413
        );
        let query = framed(
            "POST",
            "/api/mind/interpret?prompt=private",
            body.as_bytes(),
            "",
        );
        assert_eq!(status(&exchange(Arc::clone(&state), &query)), 400);

        let long_header = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\nX-Fill: {}\r\n\r\n",
            "x".repeat(MAX_HEADER_LINE_BYTES + 1)
        );
        assert_eq!(
            status(&exchange(Arc::clone(&state), long_header.as_bytes())),
            400
        );
        let long_request = format!(
            "POST /api/mind/{} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n",
            "x".repeat(MAX_REQUEST_LINE_BYTES)
        );
        assert_eq!(
            status(&exchange(Arc::clone(&state), long_request.as_bytes())),
            400
        );
        let many_headers = format!(
            "POST /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\n{}\r\n",
            (0..MAX_HEADER_COUNT)
                .map(|index| format!("X-{index}: value\r\n"))
                .collect::<String>()
        );
        assert_eq!(status(&exchange(state, many_headers.as_bytes())), 400);
        assert_eq!(engine.probes.load(Ordering::SeqCst), 0);
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn semantic_targets_and_methods_fail_closed_without_engine_calls() {
        let temp = fixture_dir();
        std::fs::write(temp.path().join("projects/0000000000000001.json"), b"{}")
            .expect("invalid export");
        let engine = MockEngine::available();
        let state = test_state(temp.path(), Arc::clone(&engine));
        let cases = [
            serde_json::json!({"project":"0123456789ABCDEF","session":SESSION,"model":MODEL}),
            serde_json::json!({"project":"0000000000000000","session":SESSION,"model":MODEL}),
            serde_json::json!({"project":"0000000000000001","session":SESSION,"model":MODEL}),
            serde_json::json!({"project":PROJECT,"session":"unknown-session","model":MODEL}),
            serde_json::json!({"project":PROJECT,"session":SESSION,"model":"other"}),
        ];
        for value in cases {
            let body = value.to_string();
            let request = framed("POST", "/api/mind/interpret", body.as_bytes(), "");
            assert_eq!(status(&exchange(Arc::clone(&state), &request)), 422);
        }
        let wrong_method = format!("GET /api/mind/interpret HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        assert_eq!(
            status(&exchange(Arc::clone(&state), wrong_method.as_bytes())),
            405
        );
        assert_eq!(engine.probes.load(Ordering::SeqCst), 0);
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 0);

        state.busy.store(true, Ordering::SeqCst);
        let refresh = format!("POST /api/refresh HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n");
        assert_eq!(status(&exchange(state, refresh.as_bytes())), 409);
    }

    #[test]
    fn successful_interpretation_is_cached_without_mutating_fixture_or_analysis() {
        let temp = fixture_dir();
        let fixture = temp.path().join(format!("projects/{PROJECT}.json"));
        let before = std::fs::read(&fixture).expect("before");
        let engine = MockEngine::available();
        let state = test_state(temp.path(), Arc::clone(&engine));
        let body = valid_body();
        let request = framed("POST", "/api/mind/interpret", body.as_bytes(), "");

        let fresh = exchange(Arc::clone(&state), &request);
        assert_eq!(status(&fresh), 200);
        let fresh_json = response_json(&fresh);
        assert_eq!(fresh_json["cached"], false);
        let cached = exchange(Arc::clone(&state), &request);
        assert_eq!(status(&cached), 200);
        let cached_json = response_json(&cached);
        assert_eq!(cached_json["cached"], true);
        assert_eq!(fresh_json["analysis"], cached_json["analysis"]);
        assert_eq!(
            fresh_json["analysis"]["output_digest"],
            cached_json["analysis"]["output_digest"]
        );
        assert_eq!(
            fresh_json["analysis"]["facts"],
            cached_json["analysis"]["facts"]
        );
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 1);
        assert_eq!(engine.probes.load(Ordering::SeqCst), 1);
        assert_eq!(state.mind.cache.lock().expect("cache").len(), 1);
        assert_eq!(std::fs::read(fixture).expect("after"), before);
    }

    #[test]
    fn concurrent_miss_conflicts_before_probe_and_guard_clears() {
        let temp = fixture_dir();
        let engine = MockEngine::available();
        let block = Arc::new(BlockControl::default());
        *engine.block.lock().expect("block slot") = Some(Arc::clone(&block));
        let state = test_state(temp.path(), Arc::clone(&engine));
        let body = valid_body();
        let request = framed("POST", "/api/mind/interpret", body.as_bytes(), "");
        let first_state = Arc::clone(&state);
        let first_request = request.clone();
        let first = std::thread::spawn(move || exchange(first_state, &first_request));
        block.wait_until_entered();

        let second = exchange(Arc::clone(&state), &request);
        assert_eq!(status(&second), 409);
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 1);
        assert_eq!(engine.probes.load(Ordering::SeqCst), 1);
        block.release();
        let first = first.join().expect("first response");
        assert_eq!(status(&first), 200);
        assert!(!state.mind.in_flight.load(Ordering::SeqCst));

        let hit = exchange(state, &request);
        assert_eq!(status(&hit), 200);
        assert_eq!(response_json(&hit)["cached"], true);
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cached_hit_is_served_while_a_different_miss_is_in_flight() {
        let temp = fixture_dir();
        const OTHER_PROJECT: &str = "0000000000000002";
        std::fs::write(
            temp.path().join(format!("projects/{OTHER_PROJECT}.json")),
            representative(),
        )
        .expect("other fixture");
        let engine = MockEngine::available();
        let state = test_state(temp.path(), Arc::clone(&engine));
        let first_body = valid_body();
        let first_request = framed("POST", "/api/mind/interpret", first_body.as_bytes(), "");
        assert_eq!(status(&exchange(Arc::clone(&state), &first_request)), 200);

        let block = Arc::new(BlockControl::default());
        *engine.block.lock().expect("block slot") = Some(Arc::clone(&block));
        let other_body = serde_json::json!({
            "project":OTHER_PROJECT,"session":SESSION,"model":MODEL
        })
        .to_string();
        let other_request = framed("POST", "/api/mind/interpret", other_body.as_bytes(), "");
        let miss_state = Arc::clone(&state);
        let miss = std::thread::spawn(move || exchange(miss_state, &other_request));
        block.wait_until_entered();

        let hit = exchange(Arc::clone(&state), &first_request);
        assert_eq!(status(&hit), 200);
        assert_eq!(response_json(&hit)["cached"], true);
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 2);
        block.release();
        assert_eq!(status(&miss.join().expect("miss response")), 200);
    }

    #[test]
    fn failures_and_partial_results_are_not_cached_and_error_guard_resets() {
        let temp = fixture_dir();
        let engine = MockEngine::available();
        engine.failures.store(1, Ordering::SeqCst);
        let state = test_state(temp.path(), Arc::clone(&engine));
        let body = valid_body();
        let request = framed("POST", "/api/mind/interpret", body.as_bytes(), "");
        let failed = exchange(Arc::clone(&state), &request);
        assert_eq!(status(&failed), 503);
        assert!(!failed.contains("PRIVATE_PROVIDER_FAILURE_DETAIL"));
        assert_eq!(state.mind.cache.lock().expect("cache").len(), 0);
        assert!(!state.mind.in_flight.load(Ordering::SeqCst));
        assert_eq!(status(&exchange(Arc::clone(&state), &request)), 200);
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 2);

        let partial_engine = MockEngine::available();
        partial_engine.partials.store(1, Ordering::SeqCst);
        let partial_state = test_state(temp.path(), Arc::clone(&partial_engine));
        assert_eq!(status(&exchange(Arc::clone(&partial_state), &request)), 503);
        assert_eq!(partial_state.mind.cache.lock().expect("cache").len(), 0);
        assert_eq!(status(&exchange(partial_state, &request)), 200);
        assert_eq!(partial_engine.analyses.load(Ordering::SeqCst), 2);

        let flag = AtomicBool::new(false);
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = InFlightGuard::acquire(&flag).expect("guard");
            panic!("synthetic unwind");
        }));
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn cache_key_axes_capacity_and_analysis_validator_are_fail_closed() {
        let catalog = catalog_from_export(representative(), PROJECT, SESSION).expect("catalog");
        let engine = MockEngine::available();
        let analysis = engine.analyze(&catalog).expect("analysis");
        assert!(complete_analysis(&analysis, &catalog, representative()));
        let base = MindCacheKey::new(&catalog, "baseline-policy");
        let changed_policy = MindCacheKey::new(&catalog, "different-policy");
        assert_ne!(base, changed_policy);
        let mut changed = base.clone();
        changed.source_digest = "0".repeat(64);
        assert_ne!(base, changed);
        changed = base.clone();
        changed.session = "other-session".to_string();
        assert_ne!(base, changed);
        changed = base.clone();
        changed.contract = "other-contract".to_string();
        assert_ne!(base, changed);
        changed = base.clone();
        changed.prompt = "other-prompt".to_string();
        assert_ne!(base, changed);
        changed = base.clone();
        changed.schema = "other-schema".to_string();
        assert_ne!(base, changed);
        changed = base.clone();
        changed.model = "other-model".to_string();
        assert_ne!(base, changed);
        changed = base.clone();
        changed.model_digest = "1".repeat(64);
        assert_ne!(base, changed);

        let mut cache = MindCache::default();
        let mut first = None;
        let mut last = None;
        for index in 0..17u64 {
            let mut key = base.clone();
            key.project = format!("{index:016x}");
            if index == 0 {
                first = Some(key.clone());
            }
            if index == 16 {
                last = Some(key.clone());
            }
            cache.insert(key, analysis.clone());
        }
        assert_eq!(cache.len(), 16);
        assert!(cache.get(&first.expect("first")).is_none());
        let cached = cache.get(&last.expect("last")).expect("last cached");
        assert_eq!(
            serde_json::to_vec(&cached).expect("cached bytes"),
            serde_json::to_vec(&analysis).expect("analysis bytes")
        );

        let mut reordered = analysis.clone();
        reordered.jobs.reverse();
        assert!(!complete_analysis(&reordered, &catalog, representative()));
        let mut wrong_scope = analysis.clone();
        wrong_scope.jobs[0].scope = "live".to_string();
        assert!(!complete_analysis(&wrong_scope, &catalog, representative()));
        let mut wrong_facts = analysis.clone();
        wrong_facts.facts.pop();
        assert!(!complete_analysis(&wrong_facts, &catalog, representative()));
        let mut empty_success = analysis.clone();
        empty_success.jobs[0].result = GroundedJobResult::Findings {
            findings: Vec::new(),
        };
        assert!(!complete_analysis(
            &empty_success,
            &catalog,
            representative()
        ));
        let mut forged_digest = analysis.clone();
        forged_digest.jobs[0].result = GroundedJobResult::Abstained {
            reason: "forged_but_bounded".to_string(),
        };
        assert!(!analysis_digest_is_valid(&forged_digest));
        assert!(!complete_analysis(
            &forged_digest,
            &catalog,
            representative()
        ));
        let mut old_contract = analysis.clone();
        old_contract.contract_version = "grounded-mind-v1".to_string();
        old_contract.output_digest = grounded_output_digest(&old_contract).expect("old digest");
        assert!(analysis_digest_is_valid(&old_contract));
        assert!(!complete_analysis(
            &old_contract,
            &catalog,
            representative()
        ));
        let mut unknown_link = analysis;
        unknown_link.jobs[0].result = GroundedJobResult::Findings {
            findings: vec![GroundedFinding {
                id: format!("row_{}", "0".repeat(20)),
                category: FindingCategory::Happened,
                text: "Recorded at export.".to_string(),
                fact_ids: vec!["fact_0000000000000000".to_string()],
                receipts: Vec::new(),
            }],
        };
        assert!(!complete_analysis(
            &unknown_link,
            &catalog,
            representative()
        ));

        let mut source: serde_json::Value =
            serde_json::from_slice(representative()).expect("source");
        source["receipts"]["other-node"] = serde_json::json!({
            "claim":{"session":"different-session","text":"other"},
            "evidence":[],"verdict":{},"ledger":{}
        });
        let source = serde_json::to_vec(&source).expect("source bytes");
        let catalog = catalog_from_export(&source, PROJECT, SESSION).expect("catalog");
        let mut cross_session = engine.analyze(&catalog).expect("analysis");
        cross_session.jobs[0].result = GroundedJobResult::Findings {
            findings: vec![GroundedFinding {
                id: format!("row_{}", "0".repeat(20)),
                category: FindingCategory::Happened,
                text: "Recorded at export.".to_string(),
                fact_ids: vec![cross_session.facts[0].id.clone()],
                receipts: vec![ReceiptHandle {
                    handle: format!("receipt_{}", "0".repeat(16)),
                    node: "other-node".to_string(),
                }],
            }],
        };
        assert!(!complete_analysis(&cross_session, &catalog, &source));
    }

    #[test]
    fn local_verification_requires_enablement_origin_and_fixed_accepted_inputs() {
        let temp = fixture_dir();
        let engine = MockEngine::available();
        let mut state = test_state(temp.path(), engine);
        let body = br#"{"action":"run","check":"approved","digest":"reviewed"}"#;
        let origin = format!("Origin: http://{ORIGIN}\r\n");
        assert_eq!(
            status(&exchange(
                Arc::clone(&state),
                &framed("POST", "/api/verification", body, "")
            )),
            403
        );
        assert_eq!(
            status(&exchange(
                Arc::clone(&state),
                &framed("POST", "/api/verification", body, &origin)
            )),
            403
        );
        Arc::get_mut(&mut state)
            .expect("single state owner")
            .verification = Some(LocalVerification {
            repo: temp.path().to_path_buf(),
            db: temp.path().join("exec.db"),
            job: Mutex::new(LocalJob::default()),
        });
        for body in [
            br#"{"action":"run","repo":"/another/project"}"#.as_slice(),
            br#"{"action":"run","argv":["arbitrary","command"]}"#.as_slice(),
            br#"{"action":"run"}"#.as_slice(),
        ] {
            assert_eq!(
                status(&exchange(
                    Arc::clone(&state),
                    &framed("POST", "/api/verification", body, &origin)
                )),
                400
            );
        }
        assert_eq!(
            status(&exchange(
                Arc::clone(&state),
                &framed(
                    "POST",
                    "/api/verification",
                    br#"{"action":"cancel"}"#,
                    &origin
                )
            )),
            409
        );
        assert!(
            !state
                .verification
                .as_ref()
                .expect("configured")
                .job
                .lock()
                .expect("job")
                .running
        );
    }

    #[test]
    fn local_draft_review_accepts_exact_revisions_without_executing_commands() {
        let temp = fixture_dir();
        let repo = temp.path().join("project");
        std::fs::create_dir_all(repo.join(".thruline")).unwrap();
        assert!(std::process::Command::new("git")
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .unwrap()
            .success());
        let db = temp.path().join("exec.db");
        let mut document = serde_json::json!({"version":1,"id":"reviewed-project","revision":1,"objective":"Review private/secret.txt before execution","protected_paths":[],"criteria":[{"id":"result","description":"The explicitly chosen command must be observed","check_ids":["command"],"human_acceptance_required":false}],"checks":[{"id":"command","category":"test","argv":["/bin/echo","not executed by acceptance"],"cwd":".","runner":"captured","mode":"captured","timeout_seconds":2,"artifacts":[]}]});
        let draft = repo.join(".thruline/acceptance.json");
        std::fs::write(&draft, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        let capture = Ledger::open(temp.path().join("ledger.db")).unwrap();
        thruline_core::capture_hold::record_for_person(
            &capture,
            thruline_core::capture_hold::Act::Excluded,
            &thruline_core::capture_hold::Hold {
                project_id: "reviewed-project",
                project_folder: repo.to_str().unwrap(),
                actor: "synthetic person",
                recorded_at: "2026-10-08T00:00:00Z",
            },
            Some("private"),
        )
        .unwrap();
        drop(capture);
        let mut state = test_state(temp.path(), MockEngine::available());
        Arc::get_mut(&mut state).unwrap().verification = Some(LocalVerification {
            repo: repo.clone(),
            db: db.clone(),
            job: Mutex::new(LocalJob::default()),
        });
        let origin = format!("Origin: http://{ORIGIN}\r\n");
        let read = || {
            response_json(&exchange(
                Arc::clone(&state),
                format!("GET /api/verification HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
            ))
        };
        let post = |value: serde_json::Value| {
            exchange(
                Arc::clone(&state),
                &framed(
                    "POST",
                    "/api/verification",
                    &serde_json::to_vec(&value).unwrap(),
                    &origin,
                ),
            )
        };
        let initial = read();
        assert!(!initial.to_string().contains("private/secret.txt"));
        assert!(initial["draft"]["objective"]
            .as_str()
            .unwrap()
            .contains("left out by your exclusion"));
        assert_eq!(initial["accepted"], false);
        assert_eq!(
            status(&post(
                serde_json::json!({"action":"accept","digest":"changed"})
            )),
            422
        );
        assert_eq!(
            status(&post(
                serde_json::json!({"action":"accept","digest":initial["draft_digest"]})
            )),
            200
        );
        let accepted = read();
        assert_eq!(accepted["accepted"], true);
        assert!(accepted["runs"].as_array().unwrap().is_empty());
        document["revision"] = serde_json::json!(2);
        document["objective"] = serde_json::json!("A subsequent reviewed requirement");
        std::fs::write(&draft, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        let changed = read();
        assert_eq!(changed["pending_draft_change"], true);
        assert_eq!(
            status(&post(
                serde_json::json!({"action":"accept","digest":initial["draft_digest"],"supersedes":accepted["acceptance_hash"]})
            )),
            422
        );
        assert_eq!(
            status(&post(
                serde_json::json!({"action":"accept","digest":changed["draft_digest"]})
            )),
            422
        );
        assert_eq!(
            status(&post(
                serde_json::json!({"action":"accept","digest":changed["draft_digest"],"supersedes":accepted["acceptance_hash"]})
            )),
            200
        );
        let final_read = read();
        assert_eq!(final_read["contract"]["revision"], 2);
        assert!(final_read["runs"].as_array().unwrap().is_empty());
        let ledger = thruline_ledger::Ledger::open_read_only(&db).unwrap();
        assert_eq!(
            ledger
                .acceptance_history(&thruline_ledger::RepoId::from_key(&repo.to_string_lossy()))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn request_shape_and_internal_error_are_bounded() {
        let request = InterpretRequest {
            project: PROJECT.to_string(),
            session: SESSION.to_string(),
            model: MODEL.to_string(),
        };
        let value = serde_json::to_value(&request).expect("request JSON");
        let keys: BTreeSet<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, BTreeSet::from(["model", "project", "session"]));
        for forbidden in ["prompt", "transcript", "evidence", "url", "path", "options"] {
            assert!(value.get(forbidden).is_none());
        }

        let temp = fixture_dir();
        let engine = MockEngine::available();
        let state = test_state(temp.path(), Arc::clone(&engine));
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = state.mind.cache.lock().expect("cache lock");
            panic!("poison cache");
        }));
        let body = valid_body();
        let request = framed("POST", "/api/mind/interpret", body.as_bytes(), "");
        let response = exchange(state, &request);
        assert_eq!(status(&response), 500);
        assert_eq!(response_json(&response)["error"], "internal_error");
        assert!(!response.contains("poison"));
        assert_eq!(engine.analyses.load(Ordering::SeqCst), 0);
    }

    /// P6-5 — the run route's five guards, its one-job rule and its outcomes.
    mod run_route_guards;

    /// C46 — Settings permission rows and their exact-folder write guards.
    mod run_permission_settings;
}

// Explicitly enabled local verification. Browser requests can select only an
// already accepted check; they cannot supply a command, repository or store.
struct LocalVerification {
    repo: PathBuf,
    db: PathBuf,
    job: Mutex<LocalJob>,
}
#[derive(Default)]
struct LocalJob {
    running: bool,
    check: Option<String>,
    cancel: Arc<AtomicBool>,
    run_id: Option<String>,
    error: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerificationRequest {
    action: String,
    check: Option<String>,
    digest: Option<String>,
    run: Option<String>,
    supersedes: Option<String>,
}
fn verification_job(v: &LocalVerification) -> Result<serde_json::Value> {
    let job = v
        .job
        .lock()
        .map_err(|_| anyhow::anyhow!("verification job lock unavailable"))?;
    Ok(
        serde_json::json!({"running":job.running,"check":job.check,"cancellation_requested":job.cancel.load(Ordering::SeqCst),"run_id":job.run_id,"error":job.error}),
    )
}

fn verification_policy(state: &State, v: &LocalVerification) -> Result<ExclusionPolicy> {
    integration_policy_for_session(state, "", Some(&v.repo))
}

fn verification_presented_json<T: Serialize>(
    stream: &mut TcpStream,
    status: u16,
    value: &T,
    policy: &ExclusionPolicy,
) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    match present_verification_action(&body, policy) {
        Ok(served) => respond(stream, status, "application/json", &served),
        Err(_) => change_action_policy_unavailable(stream),
    }
}

fn verification_read(state: &Arc<State>, stream: &mut TcpStream) -> Result<()> {
    let Some(v) = &state.verification else {
        return respond(
            stream,
            200,
            "application/json",
            &serde_json::to_vec(
                &serde_json::json!({"enabled":false,"coverage":crate::verification_pack::coverage()?,"instruction":"Start thruline ui --verification-project PROJECT --exec-db STORE to enable local verification for that project."}),
            )?,
        );
    };
    // Serialize expensive snapshots, but never hold the job mutex while reading
    // evidence. Cancel must remain available even during a status refresh.
    let Ok(_read) = state.refreshing.try_lock() else {
        return respond(
            stream,
            503,
            "application/json",
            br#"{"error":"verification_read_busy"}"#,
        );
    };
    let policy = match verification_policy(state, v) {
        Ok(policy) => policy,
        Err(problem) if error_is_store_held(&problem) => return store_held(state, stream),
        Err(_) => return change_action_policy_unavailable(stream),
    };
    let before_job = verification_job(v)?;
    match crate::verification_pack::status(&v.repo, &v.db) {
        Ok(mut value) => {
            let after_job = verification_job(v)?;
            // A status read may span the append of a finish event. Never pair
            // its earlier incomplete record with a later "job finished" flag.
            // The shared UI reader retries contention within its total deadline.
            if before_job != after_job {
                return respond(
                    stream,
                    503,
                    "application/json",
                    br#"{"error":"verification_read_busy"}"#,
                );
            }
            value["enabled"] = serde_json::json!(true);
            value["job"] = after_job;
            verification_presented_json(stream, 200, &value, &policy)
        }
        Err(error) => verification_presented_json(
            stream,
            500,
            &serde_json::json!({"error":"verification_read_incomplete","detail":format!("{error:#}"),"state":"unknown"}),
            &policy,
        ),
    }
}

fn verification_action_success<T: Serialize>(
    stream: &mut TcpStream,
    value: &T,
    policy: &ExclusionPolicy,
) -> Result<()> {
    verification_presented_json(stream, 200, value, policy)
}

fn present_verification_action<'a>(
    body: &'a [u8],
    policy: &ExclusionPolicy,
) -> Result<Cow<'a, [u8]>> {
    crate::exclusion_serve::bytes(body, policy)
}

#[cfg(test)]
mod verification_action_exclusion_tests {
    use super::*;
    use thruline_core::capture_hold::ExclusionView;

    #[test]
    fn verification_status_receipt_and_error_details_hide_excluded_paths() {
        let policy = ExclusionPolicy::from_exclusions(
            "/synthetic/project",
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
        .expect("synthetic policy");
        for raw in [
            br#"{"draft":{"objective":"Review private/secret.txt"}}"#.as_slice(),
            br#"{"run_id":"run_1","verification":{"scope":["private/secret.txt"],"explanation":"Read private/secret.txt"}}"#.as_slice(),
            br#"{"error":"receipt_unavailable","detail":"private/secret.txt is missing"}"#.as_slice(),
            br#"{"error":"draft_not_accepted","detail":"private/secret.txt changed"}"#.as_slice(),
        ] {
            let served = present_verification_action(raw, &policy).expect("presentation");
            let body = String::from_utf8(served.into_owned()).expect("UTF-8 JSON");
            assert!(!body.contains("private/secret.txt"), "{body}");
            assert!(body.contains("left out by your exclusion"), "{body}");
        }
        let acceptance = br#"{"accepted":true,"digest":"reviewed","revision":1}"#;
        assert!(matches!(
            present_verification_action(acceptance, &policy).expect("acceptance presentation"),
            Cow::Borrowed(bytes) if bytes == acceptance
        ));
    }
}

fn verification_action(state: &Arc<State>, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    if state.shutting_down.load(Ordering::SeqCst) {
        return respond(
            stream,
            503,
            "application/json",
            br#"{"error":"workspace_is_shutting_down"}"#,
        );
    }
    let Some(v) = &state.verification else {
        return respond(
            stream,
            403,
            "application/json",
            br#"{"error":"local_execution_not_enabled"}"#,
        );
    };
    let request: VerificationRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => {
            return respond(
                stream,
                400,
                "application/json",
                br#"{"error":"invalid_verification_request"}"#,
            )
        }
    };
    // A receipt and an acceptance can carry path-bearing evidence. Resolve
    // presentation policy before acceptance changes the recorded state.
    let presentation_policy = if matches!(request.action.as_str(), "receipt" | "accept") {
        match verification_policy(state, v) {
            Ok(policy) => Some(policy),
            Err(problem) if error_is_store_held(&problem) => return store_held(state, stream),
            Err(_) => return change_action_policy_unavailable(stream),
        }
    } else {
        None
    };
    if request.action == "receipt" {
        let Some(run) = request.run else {
            return respond(
                stream,
                400,
                "application/json",
                br#"{"error":"run_required"}"#,
            );
        };
        return match crate::verification_pack::receipt(&v.repo, &v.db, &run, true) {
            Ok(receipt) => verification_action_success(
                stream,
                &receipt,
                presentation_policy
                    .as_ref()
                    .expect("receipt policy resolved"),
            ),
            Err(error) => verification_presented_json(
                stream,
                422,
                &serde_json::json!({"error":"receipt_unavailable","detail":error.to_string()}),
                presentation_policy
                    .as_ref()
                    .expect("receipt policy resolved"),
            ),
        };
    }
    let mut job = v
        .job
        .lock()
        .map_err(|_| anyhow::anyhow!("verification job lock unavailable"))?;
    if request.action == "cancel" {
        if !job.running {
            return respond(
                stream,
                409,
                "application/json",
                br#"{"error":"no_running_check"}"#,
            );
        }
        job.cancel.store(true, Ordering::SeqCst);
        return respond(stream,200,"application/json",br#"{"cancellation_requested":true,"message":"Waiting for bounded capture to stop and record an incomplete attempt."}"#);
    }
    if request.action == "accept" {
        if job.running {
            return respond(
                stream,
                409,
                "application/json",
                br#"{"error":"a_check_is_already_running"}"#,
            );
        }
        let Some(digest) = request.digest else {
            return respond(
                stream,
                400,
                "application/json",
                br#"{"error":"reviewed_draft_digest_required"}"#,
            );
        };
        if request.check.is_some() || request.run.is_some() {
            return respond(
                stream,
                400,
                "application/json",
                br#"{"error":"invalid_acceptance_request"}"#,
            );
        }
        return match crate::acceptance_cmd::accept_reviewed(
            &v.repo,
            &v.db,
            &v.repo.join(".thruline/acceptance.json"),
            &digest,
            request.supersedes.as_deref(),
            "local-developer",
        ) {
            Ok(value) => verification_action_success(
                stream,
                &value,
                presentation_policy
                    .as_ref()
                    .expect("acceptance policy resolved"),
            ),
            Err(error) => verification_presented_json(
                stream,
                422,
                &serde_json::json!({
                    "error":"draft_not_accepted", "detail":format!("{error:#}")
                }),
                presentation_policy
                    .as_ref()
                    .expect("acceptance policy resolved"),
            ),
        };
    }
    if request.action != "run" {
        return respond(
            stream,
            400,
            "application/json",
            br#"{"error":"unknown_action"}"#,
        );
    }
    if state.shutting_down.load(Ordering::SeqCst) {
        return respond(
            stream,
            503,
            "application/json",
            br#"{"error":"workspace_is_shutting_down"}"#,
        );
    }
    if job.running {
        return respond(
            stream,
            409,
            "application/json",
            br#"{"error":"a_check_is_already_running"}"#,
        );
    }
    let (Some(check), Some(digest)) = (request.check, request.digest) else {
        return respond(
            stream,
            400,
            "application/json",
            br#"{"error":"accepted_check_and_digest_required"}"#,
        );
    };
    let cancel = Arc::new(AtomicBool::new(false));
    *job = LocalJob {
        running: true,
        check: Some(check.clone()),
        cancel: Arc::clone(&cancel),
        run_id: None,
        error: None,
    };
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        if let Some(v) = &state.verification {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::verification_pack::run_accepted(&v.repo, &v.db, &check, &digest, &cancel)
            }));
            if let Ok(mut job) = v.job.lock() {
                job.running = false;
                match result {
                    Ok(Ok(run))=>job.run_id=Some(run),
                    Ok(Err(error))=>job.error=Some(error.to_string()),
                    Err(_)=>job.error=Some("Verification was interrupted unexpectedly. No successful result was admitted.".into()),
                }
            }
        }
    });
    respond(stream,202,"application/json",br#"{"started":true,"message":"One accepted check requested; completion is not yet established."}"#)
}
