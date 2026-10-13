//! The whole-corpus export, with progress.
//!
//! Extracted from `Command::Export { all: true }` so `thruline ui` and
//! `thruline export --all` run the *same* code rather than two drifting copies.
//! The only thing added is a progress callback: over a real corpus this walk
//! takes minutes, and a command that prints nothing for minutes is
//! indistinguishable from a hang (Rule 12).
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use thruline_core::capture_exclusion::LEFT_OUT_FIELD;
use thruline_core::ledger::Ledger;

/// What the caller is told while the walk runs. The CLI renders these as a
/// per-project line; the server counts them.
pub enum Progress<'a> {
    /// The project list is known. Nothing has been derived yet.
    Starting { total: usize },
    /// One project finished deriving and its fixture is on disk.
    Project {
        done: usize,
        total: usize,
        id: &'a str,
        elapsed: Duration,
    },
}

/// A186 Part 0 — where the twelve minutes actually go.
///
/// A143 measured the cold export at 746 s against a ~60 s bar and recorded the
/// shape it could see from the progress line alone: the first 28 projects in
/// 12.8 s, then one project taking ~5.5 minutes by itself. That is a long tail,
/// but a progress line cannot say *which phase* of a project is expensive, and
/// the right fix differs completely depending on the answer — derivation
/// dominance invites caching, write dominance invites something else entirely.
///
/// So this decomposes the walk per phase and per project. It is **off unless
/// `THRULINE_EXPORT_PROFILE` names a path**, costs a handful of `Instant::now`
/// calls when on, and adds no CLI surface: a measurement harness is not a
/// product feature (Rule 2), and it must be re-runnable *identically* after the
/// fix or the before/after comparison is worthless.
#[derive(Default)]
struct Profile {
    rows: Vec<serde_json::Value>,
    setup_config_ms: f64,
    setup_ledger_open_ms: f64,
    setup_now_ms: f64,
    setup_project_ids_ms: f64,
    index_build_ms: f64,
    index_write_ms: f64,
}

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1000.0 * 1000.0).round() / 1000.0
}

impl Profile {
    fn enabled() -> Option<PathBuf> {
        std::env::var_os("THRULINE_EXPORT_PROFILE").map(PathBuf::from)
    }

    /// Fail loud (Rule 12): a profile that cannot be written is reported, never
    /// swallowed — a silent miss would leave a "measured" claim with no artifact.
    fn write(&self, path: &Path, total_ms: f64, projects: usize) -> Result<()> {
        let doc = serde_json::json!({
            "probe": "c-a186-export-profile",
            "phases_setup_ms": {
                "config": self.setup_config_ms,
                "ledger_open": self.setup_ledger_open_ms,
                "now_queues": self.setup_now_ms,
                "project_ids": self.setup_project_ids_ms,
            },
            "phases_index_ms": {
                "build": self.index_build_ms,
                "write": self.index_write_ms,
            },
            "total_ms": total_ms,
            "projects": projects,
            "per_project": self.rows,
        });
        let body = serde_json::to_string_pretty(&doc)?;
        std::fs::write(path, body + "\n")
            .with_context(|| format!("writing the export profile to {}", path.display()))
    }
}

pub struct ExportAllReport {
    pub projects: usize,
    pub needing_attention: u64,
    pub index_path: PathBuf,
    pub index_bytes: u64,
    pub elapsed: Duration,
}

/// Derive and write every project fixture plus the §9 summary index.
///
/// `dir` is taken as given — resolving a default belongs to the caller, because
/// the two callers have different right answers (the repo's `ui/public/fixtures`
/// for `export --all`, `~/.thruline/export` for `ui`).
/// The one stamp in the export that is not derived from the record.
///
/// `generated_at` says when this reading was taken, so by default it is the
/// wall clock and nothing else. That makes two exports of an unchanged store
/// differ by exactly one field whenever they straddle a second, which is fine
/// for a person reading a date and fatal for a test that asks whether a
/// migration changed any answer. `--generated-at` lets the caller pin it, and
/// it reaches **only** this field: every other byte in the export still comes
/// from the record.
///
/// Refused unless it is the canonical UTC form the export already writes,
/// `YYYY-MM-DDTHH:MM:SSZ` — a stamp the reader cannot parse is worse than a
/// wrong one, because the file still looks like an export (Rule 12).
pub fn pinned_generated_at(raw: &str) -> Result<String> {
    fn digits(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
    }
    let bad = || {
        anyhow::anyhow!(
            "`--generated-at {raw}` is not a time this export can write.\n\
             Use the form the export itself uses: 2026-09-17T14:05:00Z — four-digit year, \
             two digits each for month, day, hour, minute and second, UTC, ending in Z.\n\
             Leave the flag off and the export stamps the moment it ran."
        )
    };
    let b = raw.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return Err(bad());
    }
    let field = |from: usize, to: usize| -> Option<u32> {
        let part = raw.get(from..to)?;
        if !digits(part) {
            return None;
        }
        part.parse().ok()
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        field(0, 4),
        field(5, 7),
        field(8, 10),
        field(11, 13),
        field(14, 16),
        field(17, 19),
    ) else {
        return Err(bad());
    };
    let ok = (1970..=9999).contains(&year)
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 59;
    if !ok {
        return Err(bad());
    }
    Ok(raw.to_string())
}

pub fn export_all(
    db: &Path,
    dir: &Path,
    diagnostics: bool,
    pinned: Option<&str>,
    progress: &mut dyn FnMut(Progress),
) -> Result<ExportAllReport> {
    export_all_with_presentation(db, dir, diagnostics, pinned, progress, Presentation::Export)
}

/// The UI's private staging remains unfiltered so an include can restore a
/// previously hidden entry from the same bytes without a full corpus refresh.
/// `/fixtures` and the local-model route filter this staging at serve time.
pub(crate) fn export_all_staging(
    db: &Path,
    dir: &Path,
    diagnostics: bool,
    pinned: Option<&str>,
    progress: &mut dyn FnMut(Progress),
) -> Result<ExportAllReport> {
    export_all_with_presentation(
        db,
        dir,
        diagnostics,
        pinned,
        progress,
        Presentation::Staging,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Presentation {
    Export,
    Staging,
}

fn export_all_with_presentation(
    db: &Path,
    dir: &Path,
    diagnostics: bool,
    pinned: Option<&str>,
    progress: &mut dyn FnMut(Progress),
    presentation: Presentation,
) -> Result<ExportAllReport> {
    let started = Instant::now();
    let profile_path = Profile::enabled();
    let mut profile = Profile::default();

    let mark = Instant::now();
    let cfg =
        thruline_core::config::Config::load_default().context("loading ~/.thruline/config.toml")?;
    profile.setup_config_ms = ms(mark.elapsed());

    let mark = Instant::now();
    let ledger = Ledger::open(db).context("opening the ledger")?;
    profile.setup_ledger_open_ms = ms(mark.elapsed());
    let project_roots = if presentation == Presentation::Export {
        Some(
            crate::exclusion_serve::project_roots(&ledger)
                .context("reading project roots for export exclusions")?,
        )
    } else {
        None
    };

    // The only non-derived field in the whole export, and the only thing
    // `--generated-at` can reach.
    let generated_at = match pinned {
        Some(pinned) => pinned.to_string(),
        None => thruline_core::ingest::iso8601_utc_now(),
    };

    let mark = Instant::now();
    let coverage = crate::doctor::coverage(None);
    let queues = thruline_core::now::now(&ledger, None, false, coverage).context("deriving now")?;
    profile.setup_now_ms = ms(mark.elapsed());
    let by_project: std::collections::BTreeMap<&str, &thruline_core::now::ProjectNow> =
        queues.iter().map(|q| (q.project_id.as_str(), q)).collect();

    let mark = Instant::now();
    let ids = ledger.project_ids().context("listing projects")?;
    profile.setup_project_ids_ms = ms(mark.elapsed());
    let total = ids.len();
    progress(Progress::Starting { total });

    let mut rows = Vec::with_capacity(total);
    let mut index_left_out = 0_u64;
    let mut written = 0usize;
    // Day granularity, and the newest across every project: the honesty strip
    // answers how far behind the whole reading is.
    let mut newest_record: Option<String> = None;
    for id in &ids {
        let mark = Instant::now();
        let graph = thruline_core::derive::derive_project_with(&ledger, id, &cfg)
            .with_context(|| format!("deriving {id}"))?;
        let derive_ms = ms(mark.elapsed());

        let mark = Instant::now();
        let mut doc = thruline_core::export::export_project_with_options(
            &ledger,
            &graph,
            &generated_at,
            thruline_core::export::ExportOptions { diagnostics },
        )
        .with_context(|| format!("building the §9 document for {id}"))?;
        let policy = if let Some(roots) = project_roots.as_ref() {
            let root = roots
                .get(id)
                .with_context(|| format!("project {id} has no stored root for exclusions"))?;
            let policy = crate::exclusion_serve::policy(&ledger, root)
                .with_context(|| format!("reading exclusions for {id}"))?;
            crate::exclusion_serve::checked_apply(&mut doc, &policy)
                .with_context(|| format!("applying exclusions to {id} export"))?;
            Some(policy)
        } else {
            None
        };
        let render_ms = ms(mark.elapsed());
        let Some(queue) = by_project.get(id.as_str()) else {
            // Rule 12: a project the queue does not know about is a derivation
            // disagreement, not a row to invent.
            anyhow::bail!("project {id} has no attention queue — refusing to invent a row");
        };
        for session in &graph.sessions {
            if let Some(started_at) = session.started_at.as_deref() {
                let day = started_at
                    .split('T')
                    .next()
                    .unwrap_or(started_at)
                    .to_string();
                if newest_record.as_deref().is_none_or(|n| day.as_str() > n) {
                    newest_record = Some(day);
                }
            }
        }
        let mut row = thruline_core::export::index_row(&doc, queue);
        if let Some(policy) = policy.as_ref() {
            index_left_out += crate::exclusion_serve::checked_apply(&mut row, policy)
                .with_context(|| format!("applying exclusions to {id} index row"))?;
        }
        rows.push(row);
        let path = dir.join("projects").join(format!(
            "{}.json",
            thruline_core::export::project_fixture_id(id)
        ));
        let mark = Instant::now();
        thruline_core::export::write_json(&path, &doc)
            .with_context(|| format!("writing {}", path.display()))?;
        let write_ms = ms(mark.elapsed());
        written += 1;

        // Bound as `profile_out`, NOT `path`: the loop already has a `path`
        // for the project fixture, and shadowing it here made the recorded
        // `bytes` measure the profile file instead of the document. Clippy
        // caught it as a needless borrow; it was a wrong number.
        if let Some(profile_out) = profile_path.as_deref() {
            // Counts alongside the times: a per-project cost is only
            // interpretable against the size of what it produced.
            let count = |k: &str| doc[k].as_array().map(|a| a.len()).unwrap_or(0);
            profile.rows.push(serde_json::json!({
                "project": thruline_core::export::project_fixture_id(id),
                "derive_ms": derive_ms,
                "render_ms": render_ms,
                "write_ms": write_ms,
                "total_ms": derive_ms + render_ms + write_ms,
                "nodes": count("nodes"),
                "edges": count("edges"),
                "sessions": count("sessions"),
                "bytes": std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            }));
            // Written after EVERY project, not once at the end. The first
            // profiled run of this walk died at project 28 of 91 and produced
            // no artifact at all, because the write was on the success path —
            // a profiler that only reports when nothing went wrong cannot
            // measure the case worth measuring (Rule 12).
            profile.write(profile_out, ms(started.elapsed()), written)?;
        }
        progress(Progress::Project {
            done: written,
            total,
            id,
            elapsed: started.elapsed(),
        });
    }

    let mark = Instant::now();
    let mut index = thruline_core::export::export_index(
        &generated_at,
        thruline_core::export::ExportOptions { diagnostics },
        rows,
        newest_record.as_deref(),
    );
    if index_left_out > 0 {
        index[LEFT_OUT_FIELD] = serde_json::Value::from(index_left_out);
    }
    profile.index_build_ms = ms(mark.elapsed());
    let index_path = dir.join("index.json");
    let mark = Instant::now();
    thruline_core::export::write_json(&index_path, &index).context("writing the index")?;
    profile.index_write_ms = ms(mark.elapsed());

    if let Some(path) = profile_path.as_deref() {
        profile.write(path, ms(started.elapsed()), written)?;
    }

    Ok(ExportAllReport {
        projects: written,
        needing_attention: index["totals"]["needing_attention"].as_u64().unwrap_or(0),
        index_bytes: std::fs::metadata(&index_path).map(|m| m.len()).unwrap_or(0),
        index_path,
        elapsed: started.elapsed(),
    })
}
