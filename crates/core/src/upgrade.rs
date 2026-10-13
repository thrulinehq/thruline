//! A193 / Q-A191.1 — schema upgrades are paid deliberately, by name.
//!
//! The question A191 raised and could not answer: every DDL change costs a full
//! migration pass over the ledger, the cost grows with the corpus, and it is
//! charged to **whoever opens the ledger first after installing a new binary**.
//! That is very often not a person. It is a Claude Code hook with a five-second
//! timeout, or an MCP server, or an editor integration — none of which asked
//! for the work, none of which can report it, and one of which (A168's live
//! finding) simply fails.
//!
//! The ruling: migrations run **only** under `thruline upgrade`. Every other
//! command that finds a pending migration says so in one line and exits having
//! paid nothing. The refusal lives in [`crate::ledger::Ledger::init`] so that
//! every one of the ~40 `Ledger::open` call sites inherits it without a single
//! one of them having to remember.
//!
//! This module holds the parts that are not the refusal itself: what to tell a
//! person about how long it will take, and whether there is room on the disk to
//! do it.

use std::path::Path;

use serde::Serialize;

use crate::error::Result;

/// What `Ledger::open` would do with a database, established without doing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum UpgradeState {
    /// No database, or an empty file. `open` will create the schema, which is
    /// free and needs no permission.
    Fresh,
    /// The schema on disk carries this binary's fingerprint. `open` fast-paths.
    Current,
    /// An existing ledger written by a different schema. `open` refuses;
    /// `thruline upgrade` is the only thing that will pay.
    Pending { events: u64 },
}

impl UpgradeState {
    pub fn is_pending(&self) -> bool {
        matches!(self, UpgradeState::Pending { .. })
    }
}

/// Measured costs of the full schema path, each with the basis it was taken
/// under.
///
/// This is a **stamped table**, in the P2b sense: an estimate quoted from an
/// unstamped number is exactly what A186, A189 and A190 each did. The entries
/// are `(events, seconds, basis)`, ordered by events.
///
/// **A191's recorded 120 s does not reproduce and is not in this table.** It was
/// measured on the live ledger under conditions nobody wrote down — concurrent
/// readers included, since a `thruline ui` server and an MCP server were running
/// against it. Re-measured under B-A192's conditions the same corpus migrates in
/// 20–34 s. Both readings are real; only one has a basis, and only one is
/// therefore allowed to inform what a user is told.
pub const MEASURED_COSTS: &[(u64, f64, &str)] = &[
    // A real migration of a genuinely old schema (the pre-A92 backup), not a
    // re-run of no-op steps: 777,404 events, 1.47 GB, 4.17 s wall including
    // ~0.07 s of command overhead.
    (777_404, 4.1, "B-A192"),
    // The current corpus with its fingerprint cleared, so every step re-runs:
    // 2,012,204 events, 6.85 GB. Three readings — 33.7 s and 20.2 s by
    // subtraction from a timing command, and 24.3 s measured directly by
    // `thruline upgrade` itself, which is the better instrument. The SLOWEST is
    // recorded on purpose: an estimate that runs under is a broken promise,
    // and one that runs over is a pleasant surprise.
    //
    // Nearly all of it is one step. `source project identity` alone accounted
    // for **24.1 s of a 24.3 s** upgrade — which nobody could have known before
    // the progress output existed, because the old path was one silent stall.
    (2_012_204, 33.7, "B-A192"),
];

/// Seconds the full path is expected to take on a ledger of `events` events.
///
/// Linear in events between the measured points, and linear extrapolation
/// outside them. Two data points do not establish a shape, and this function
/// does not pretend otherwise — it exists so a person is told *roughly* what
/// they are about to wait for, and [`estimate_phrase`] is deliberately coarse
/// so the number is not read as a promise.
pub fn estimate_seconds(events: u64) -> f64 {
    if MEASURED_COSTS.is_empty() {
        return 0.0;
    }
    if MEASURED_COSTS.len() == 1 || events <= MEASURED_COSTS[0].0 {
        let (base_events, base_secs, _) = MEASURED_COSTS[0];
        if base_events == 0 {
            return base_secs;
        }
        return (base_secs * events as f64 / base_events as f64).max(0.5);
    }
    for pair in MEASURED_COSTS.windows(2) {
        let (e0, s0, _) = pair[0];
        let (e1, s1, _) = pair[1];
        if events <= e1 {
            let span = (e1 - e0) as f64;
            if span <= 0.0 {
                return s1;
            }
            return s0 + (s1 - s0) * (events - e0) as f64 / span;
        }
    }
    // Beyond the largest measured point: extrapolate on the last segment's
    // slope rather than clamping, so a corpus twice the size is not quoted the
    // same number as the one that was measured.
    let n = MEASURED_COSTS.len();
    let (e0, s0, _) = MEASURED_COSTS[n - 2];
    let (e1, s1, _) = MEASURED_COSTS[n - 1];
    let slope = (s1 - s0) / (e1 - e0) as f64;
    s1 + slope * (events - e1) as f64
}

/// The estimate as a person should read it: coarse, prefixed, and never
/// precise enough to be mistaken for a guarantee.
pub fn estimate_phrase(events: u64) -> String {
    let secs = estimate_seconds(events);
    if secs < 10.0 {
        "about a few seconds".to_string()
    } else if secs < 90.0 {
        format!("about {} seconds", (secs / 5.0).round() as u64 * 5)
    } else {
        format!("about {} minutes", (secs / 60.0).ceil() as u64)
    }
}

/// The basis ids behind [`MEASURED_COSTS`], for the surface that quotes them.
pub fn estimate_bases() -> Vec<&'static str> {
    let mut bases: Vec<&'static str> = MEASURED_COSTS.iter().map(|(_, _, b)| *b).collect();
    bases.sort_unstable();
    bases.dedup();
    bases
}

/// What the disk preflight found.
#[derive(Debug, Clone, Serialize)]
pub struct DiskPreflight {
    pub ledger_bytes: u64,
    pub available_bytes: Option<u64>,
    /// Room enough by the rule below. `None` available space means the check
    /// could not run, which is reported rather than treated as a pass.
    pub sufficient: Option<bool>,
}

/// Require free space at least the size of the ledger itself (A131).
///
/// The rule is conservative and the reason is stated rather than assumed: a
/// migration step may rewrite a table, SQLite does that by building a copy, and
/// this code cannot know in advance whether a *future* migration will. A131 is
/// on the record because a runaway ingest took this very volume down to 120 MiB
/// free; the cost of being wrong in the generous direction is a message, and
/// the cost of being wrong in the other direction is a half-written ledger.
pub fn disk_preflight(ledger: &Path) -> DiskPreflight {
    let ledger_bytes = std::fs::metadata(ledger).map(|m| m.len()).unwrap_or(0);
    let available_bytes = available_space(ledger);
    let sufficient = available_bytes.map(|free| free >= ledger_bytes);
    DiskPreflight {
        ledger_bytes,
        available_bytes,
        sufficient,
    }
}

/// Free bytes on the filesystem holding `path`, or `None` if it cannot be
/// established. Never guesses.
fn available_space(path: &Path) -> Option<u64> {
    let target = path
        .canonicalize()
        .ok()
        .or_else(|| path.parent().and_then(|p| p.canonicalize().ok()))?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| target.starts_with(d.mount_point()))
        // The deepest matching mount point is the one the file is actually on.
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// Bytes as a person reads them.
pub fn human_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB)
    } else {
        format!("{:.0} MiB", bytes as f64 / MIB)
    }
}

/// One table's name and how many rows it holds.
pub type TableCount = (String, u64);

/// The one table an upgrade is expected to write to: it stamps the schema it
/// applied. The census reports it by itself rather than as a difference.
pub const STAMP_TABLE: &str = "schema_migrations";

/// Tables a migration step RENAMES, as `(old name, new name)`. `RENAME TO` moves
/// no row, so the census compares the old name's count before with the new
/// name's count after; without this the one rename would read as a table that
/// lost every row and a new empty one (R-2026-09-24-22).
pub const RENAMED_TABLES: &[(&str, &str)] = &[("change_dispositions", "change_obligation_waivers")];

/// How many rows every table of a ledger holds, by table name, in name order.
///
/// A plain read of the file; it never migrates and never writes. `COUNT(*)` of
/// every table is a pass over the ledger, so `thruline upgrade` takes it once
/// before the schema path and once after, and nothing else does.
pub fn row_census(path: &Path) -> Result<Vec<TableCount>> {
    let conn = crate::store_hold::open_connection(path, rusqlite::OpenFlags::default())?;
    let names = {
        let mut statement = conn.prepare(
            "SELECT name FROM sqlite_master
              WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let mut census = Vec::with_capacity(names.len());
    for name in names {
        let quoted = name.replace('"', "\"\"");
        let rows: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |r| {
            r.get(0)
        })?;
        census.push((name, rows.max(0) as u64));
    }
    Ok(census)
}

/// What two censuses taken either side of an upgrade say.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct CensusDiff {
    /// Tables that existed before (the stamp table excepted).
    pub tables_compared: usize,
    /// Rows those tables held before.
    pub rows_compared: u64,
    /// `(rows before, rows after)` of [`STAMP_TABLE`], when it exists after.
    pub stamp: Option<(u64, u64)>,
    /// Tables that did not exist before, with the rows each holds now.
    pub new_tables: Vec<TableCount>,
    /// `(table, before, after)` where an existing table holds MORE rows now: a
    /// migration step backfilled it (the source-identity mapping, on a store
    /// older than the mapping), or another process wrote while the upgrade ran.
    pub grew: Vec<(String, u64, u64)>,
    /// `(table, before, after)` where an existing table holds FEWER rows now,
    /// or is gone. After an upgrade of a store at the previous schema this is
    /// never expected.
    pub lost: Vec<(String, u64, u64)>,
}

impl CensusDiff {
    /// Rows before = rows after for every existing table.
    pub fn unchanged(&self) -> bool {
        self.grew.is_empty() && self.lost.is_empty()
    }

    /// The new tables that hold rows.
    pub fn new_tables_holding_rows(&self) -> Vec<&TableCount> {
        self.new_tables.iter().filter(|(_, n)| *n > 0).collect()
    }
}

/// Compare the census taken before an upgrade with the one taken after it.
pub fn compare_census(before: &[TableCount], after: &[TableCount]) -> CensusDiff {
    let count_of = |census: &[TableCount], name: &str| -> Option<u64> {
        census
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, rows)| *rows)
    };
    let mut diff = CensusDiff::default();
    for (name, was) in before {
        if name == STAMP_TABLE {
            continue;
        }
        diff.tables_compared += 1;
        diff.rows_compared += was;
        let now_name = RENAMED_TABLES
            .iter()
            .find(|(old, _)| old == name)
            .map_or(name.as_str(), |(_, new)| *new);
        match count_of(after, now_name) {
            None => diff.lost.push((name.clone(), *was, 0)),
            Some(now) if now < *was => diff.lost.push((name.clone(), *was, now)),
            Some(now) if now > *was => diff.grew.push((name.clone(), *was, now)),
            Some(_) => {}
        }
    }
    if let Some(now) = count_of(after, STAMP_TABLE) {
        diff.stamp = Some((count_of(before, STAMP_TABLE).unwrap_or(0), now));
    }
    for (name, now) in after {
        let was_there =
            count_of(before, name).is_some() || RENAMED_TABLES.iter().any(|(_, new)| new == name);
        if name != STAMP_TABLE && !was_there {
            diff.new_tables.push((name.clone(), *now));
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    fn census(entries: &[(&str, u64)]) -> Vec<TableCount> {
        entries.iter().map(|(n, c)| (n.to_string(), *c)).collect()
    }

    #[test]
    fn a_census_of_an_unchanged_store_is_unchanged_and_names_the_new_tables() {
        let before = census(&[("events", 10), ("projects", 2), ("schema_migrations", 3)]);
        let after = census(&[
            ("events", 10),
            ("new_a", 0),
            ("new_b", 0),
            ("projects", 2),
            ("schema_migrations", 4),
        ]);
        let diff = compare_census(&before, &after);
        assert!(diff.unchanged());
        assert_eq!((diff.tables_compared, diff.rows_compared), (2, 12));
        assert_eq!(diff.stamp, Some((3, 4)));
        assert_eq!(diff.new_tables, census(&[("new_a", 0), ("new_b", 0)]));
        assert!(diff.new_tables_holding_rows().is_empty());
    }

    #[test]
    fn a_table_that_lost_rows_or_vanished_is_lost_and_one_that_grew_is_not() {
        let before = census(&[("a", 5), ("b", 5), ("c", 5), ("d", 5)]);
        let after = census(&[("a", 4), ("b", 6), ("c", 5)]);
        let diff = compare_census(&before, &after);
        assert!(!diff.unchanged());
        assert_eq!(
            diff.lost,
            vec![("a".to_string(), 5, 4), ("d".to_string(), 5, 0)]
        );
        assert_eq!(diff.grew, vec![("b".to_string(), 5, 6)]);
    }

    #[test]
    fn a_renamed_table_is_compared_under_its_new_name() {
        let before = census(&[("change_dispositions", 3)]);
        let renamed = census(&[("change_obligation_waivers", 3)]);
        let diff = compare_census(&before, &renamed);
        assert!(diff.unchanged(), "{diff:?}");
        assert!(diff.new_tables.is_empty(), "{diff:?}");
        let emptied = census(&[("change_obligation_waivers", 0)]);
        assert_eq!(
            compare_census(&before, &emptied).lost,
            vec![("change_dispositions".to_string(), 3, 0)]
        );
    }

    #[test]
    fn the_row_census_counts_every_table_of_a_real_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("census.db");
        {
            let conn = rusqlite::Connection::open(&path).expect("open");
            conn.execute_batch(
                "CREATE TABLE one(x); CREATE TABLE \"two words\"(y);
                 INSERT INTO one VALUES(1),(2),(3); INSERT INTO \"two words\" VALUES(9);",
            )
            .expect("fill");
        }
        assert_eq!(
            row_census(&path).expect("census"),
            census(&[("one", 3), ("two words", 1)])
        );
    }

    #[test]
    fn the_estimate_grows_with_the_corpus() {
        assert!(estimate_seconds(4_000_000) > estimate_seconds(2_012_204));
        assert!(estimate_seconds(2_012_204) > estimate_seconds(777_404));
        assert!(estimate_seconds(1_000) < estimate_seconds(777_404));
    }

    #[test]
    fn the_estimate_reproduces_its_measured_points() {
        for (events, secs, _) in MEASURED_COSTS {
            let got = estimate_seconds(*events);
            assert!(
                (got - secs).abs() < 0.01,
                "estimate at {events} events is {got}, measured {secs}"
            );
        }
    }

    #[test]
    fn the_phrase_is_coarse_enough_not_to_read_as_a_promise() {
        assert_eq!(estimate_phrase(1_000), "about a few seconds");
        assert!(estimate_phrase(2_012_204).starts_with("about 35 second"));
        assert!(estimate_phrase(20_000_000).ends_with("minutes"));
    }

    #[test]
    fn every_measured_cost_names_a_basis_that_exists() {
        let registry = crate::perf_basis::Registry::load().expect("docs/perf-basis.toml");
        for (_, _, basis) in MEASURED_COSTS {
            assert!(
                registry.basis(basis).is_some(),
                "measured migration cost cites basis {basis}, which is not in the registry"
            );
        }
    }
}
