//! The HQ load: every recording in a [`super::LoadPlan`], ingested into the
//! ledger one file at a time. `POST /api/hq/projects/<id>/load` runs this in
//! a background thread, and `tests/hq_load_speed_probe.rs` runs the same code
//! to measure it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::error::Result;
use crate::ingest::{IngestReport, LoadMirror};
use crate::ledger::Ledger;

use super::LoadPlan;

/// What one file of a load did and what it cost.
#[derive(Debug, Clone, Default)]
pub struct LoadFileTiming {
    pub file: PathBuf,
    pub codex: bool,
    /// The file's size when the load reached it (0 when it could not be stat'd).
    pub bytes: u64,
    pub wall_ns: u64,
    /// The file was found unchanged since the ledger last read it, and was
    /// not opened.
    pub skipped_unopened: bool,
    /// The file was in the plan and is no longer on disk: nothing was read
    /// and nothing changed.
    pub gone: bool,
    pub events_added: u64,
    pub sessions: u64,
    /// Ingest's own phase split (`IngestReport::phase_ns`), plus `stat`.
    pub phase_ns: BTreeMap<String, u64>,
    /// Masked, per-file sentences for what ingest refused or failed on.
    pub problems: Vec<String>,
    /// Another writer held the store for the whole busy wait.
    pub store_busy: bool,
    /// A Codex mapping made on a worker thread was used as made.
    pub worker_mapped: bool,
}

/// The whole load's totals.
#[derive(Debug, Clone, Default)]
pub struct LoadOutcome {
    pub files_done: u64,
    /// Files found unchanged and not opened.
    pub files_skipped_unopened: u64,
    /// Files that were planned and had gone by the time the load reached them.
    pub files_gone: u64,
    /// Opening the mirror and its one orphan scan.
    pub setup_ns: u64,
    pub events_added: u64,
    pub sessions_read: u64,
    pub store_busy: u64,
    pub errors: Vec<String>,
}

fn shown(file: &Path) -> String {
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn problems_of(shown: &str, report: &IngestReport) -> Vec<String> {
    let mut problems = Vec::new();
    if !report.integrity_alarms.is_empty() {
        problems.push(format!(
            "{shown}: {} line(s) changed since they were read",
            report.integrity_alarms.len()
        ));
    }
    if report.rejected_candidates > 0 {
        problems.push(format!(
            "{shown}: {} recording generation(s) not accepted ({})",
            report.rejected_candidates,
            report
                .candidate_statuses
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    problems
}

/// C34 pause gate (3 of 6): the plan, less the recordings a pause covers.
///
/// A recording whose folder is paused now is left out and counted in
/// `paused_by_you`; one whose session started inside a pause that has been
/// lifted is left out and counted in `started_while_paused`; every other file
/// stays, in the order it was found. The plan's `bytes` is recomputed over what
/// is left. With no pause in the record the plan is returned untouched and no
/// recording is opened.
pub fn apply_pause_gate(plan: LoadPlan, gate: &crate::capture_pause::PauseGate) -> LoadPlan {
    if gate.is_empty() {
        return plan;
    }
    let (claude_code, left_out_claude) = crate::capture_pause::filter_files(gate, plan.claude_code);
    let (codex, left_out_codex) = crate::capture_pause::filter_files(gate, plan.codex);
    let bytes = claude_code
        .iter()
        .chain(codex.iter())
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    LoadPlan {
        claude_code,
        codex,
        bytes,
        paused_by_you: plan.paused_by_you
            + left_out_claude.paused_by_you
            + left_out_codex.paused_by_you,
        started_while_paused: plan.started_while_paused
            + left_out_claude.started_while_paused
            + left_out_codex.started_while_paused,
    }
}

/// The order a load reads its files in: newest modification first, so the
/// newest threads land first; ties by path. Deterministic for a given set of
/// files and times — a parallel and a sequential load use the same order.
pub fn load_order(plan: &LoadPlan) -> Vec<(PathBuf, bool)> {
    let mut work: Vec<(Option<std::time::SystemTime>, PathBuf, bool)> = plan
        .claude_code
        .iter()
        .map(|p| (p, false))
        .chain(plan.codex.iter().map(|p| (p, true)))
        .map(|(p, codex)| {
            let modified = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            (modified, p.clone(), codex)
        })
        .collect();
    work.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    work.into_iter().map(|(_, p, codex)| (p, codex)).collect()
}

/// Worker threads a load uses by default: half the machine's cores, at least
/// one and at most four.
pub fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(1, 4))
        .unwrap_or(1)
}

/// [`run_load_with`] on [`default_workers`] threads.
pub fn run_load(
    ledger: &mut Ledger,
    mirror: &Path,
    plan: &LoadPlan,
    on_file: &mut dyn FnMut(&LoadFileTiming) -> bool,
) -> LoadOutcome {
    run_load_with(ledger, mirror, plan, default_workers(), on_file)
}

/// Ingest every file of `plan` into `ledger`, sealing into `mirror`, newest
/// file first ([`load_order`]). Calls `on_file` after each file, in that
/// order; returning `false` stops the load there. One file's failure never
/// stops the others.
///
/// C27:
/// * The mirror is opened and its orphan scan run once
///   ([`LoadMirror::open`]), not once per file.
/// * Before anything is read, every file is checked: one that is gone is
///   counted and skipped; one the ledger has read in full and that has not
///   changed since is skipped without being opened ([`LoadMirror::unchanged`]).
/// * The rest are ingested — with `workers == 0` one at a time, each under
///   its own mirror lease, exactly as before; otherwise with their reading
///   on `workers` threads and every ledger write on this one, in the same
///   order ([`LoadMirror::ingest_parallel`]). Each file commits on its own,
///   so the project's heads move — and HQ's index sees new rows — file by
///   file during the load.
/// * A file that grew is sealed in full and ingested as before (C26: no
///   mirror slimming in v0.2).
pub fn run_load_with(
    ledger: &mut Ledger,
    mirror: &Path,
    plan: &LoadPlan,
    workers: usize,
    on_file: &mut dyn FnMut(&LoadFileTiming) -> bool,
) -> LoadOutcome {
    let mut outcome = LoadOutcome::default();
    let t = Instant::now();
    let loader = match LoadMirror::open(ledger, mirror) {
        Ok(l) => l,
        Err(e) => {
            outcome
                .errors
                .push(crate::redact::mask_recorded(&format!("store: {e}")).0);
            return outcome;
        }
    };
    outcome.setup_ns = t.elapsed().as_nanos() as u64;

    // Decide every file before reading any, so a parallel and a sequential
    // load decide identically.
    let mut timings: Vec<LoadFileTiming> = load_order(plan)
        .into_iter()
        .map(|(file, codex)| {
            let t = Instant::now();
            let mut timing = LoadFileTiming {
                file,
                codex,
                ..LoadFileTiming::default()
            };
            let meta = std::fs::metadata(&timing.file);
            timing.bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            if matches!(&meta, Err(e) if e.kind() == std::io::ErrorKind::NotFound) {
                timing.gone = true;
            } else if loader.unchanged(ledger, &timing.file, codex) {
                timing.skipped_unopened = true;
            }
            let ns = t.elapsed().as_nanos() as u64;
            timing.phase_ns.insert("check".into(), ns);
            timing.wall_ns = ns;
            timing
        })
        .collect();
    let work: Vec<usize> = (0..timings.len())
        .filter(|&i| !timings[i].gone && !timings[i].skipped_unopened)
        .collect();

    // Report every file in order: the ones decided without reading as soon
    // as the files before them are done.
    let mut next = 0usize;
    let mut stopped = false;
    let mut emit_through = |upto: usize,
                            timings: &mut Vec<LoadFileTiming>,
                            outcome: &mut LoadOutcome,
                            next: &mut usize|
     -> bool {
        while *next < upto {
            let timing = &timings[*next];
            outcome.files_done += 1;
            outcome.files_skipped_unopened += u64::from(timing.skipped_unopened);
            outcome.files_gone += u64::from(timing.gone);
            outcome.events_added += timing.events_added;
            outcome.sessions_read += timing.sessions;
            outcome.store_busy += u64::from(timing.store_busy);
            outcome.errors.extend(timing.problems.iter().cloned());
            *next += 1;
            if !on_file(timing) {
                return false;
            }
        }
        true
    };

    let record =
        |i: usize, result: Result<IngestReport>, wall: u64, timings: &mut Vec<LoadFileTiming>| {
            let timing = &mut timings[i];
            let name = shown(&timing.file);
            timing.wall_ns += wall;
            match result {
                Ok(report) => {
                    timing.events_added = report.new_events;
                    timing.sessions = report.sessions;
                    timing.worker_mapped = report.worker_mappings_used > 0;
                    timing.problems = problems_of(&name, &report);
                    for (k, v) in &report.phase_ns {
                        *timing.phase_ns.entry((*k).to_string()).or_insert(0) += v;
                    }
                }
                Err(e) if e.is_busy() => timing.store_busy = true,
                Err(e) => timing
                    .problems
                    .push(crate::redact::mask_recorded(&format!("{name}: {e}")).0),
            }
        };

    if workers == 0 {
        for &i in &work {
            if !emit_through(i, &mut timings, &mut outcome, &mut next) {
                stopped = true;
                break;
            }
            let t = Instant::now();
            let (file, codex) = (timings[i].file.clone(), timings[i].codex);
            let result = if codex {
                loader.ingest_codex(ledger, &file)
            } else {
                loader.ingest_claude(ledger, &file)
            };
            record(i, result, t.elapsed().as_nanos() as u64, &mut timings);
        }
    } else {
        let files: Vec<(std::path::PathBuf, bool)> = work
            .iter()
            .map(|&i| (timings[i].file.clone(), timings[i].codex))
            .collect();
        let mut last = Instant::now();
        loader.ingest_parallel(ledger, &files, workers, &mut |k, result| {
            let i = work[k];
            record(i, result, last.elapsed().as_nanos() as u64, &mut timings);
            last = Instant::now();
            if !emit_through(i + 1, &mut timings, &mut outcome, &mut next) {
                stopped = true;
                return false;
            }
            true
        });
    }
    if !stopped {
        let total = timings.len();
        emit_through(total, &mut timings, &mut outcome, &mut next);
    }
    outcome
}
