//! C29 R3 — what the provenance check costs a reload's decision phase.
//! Ignored by default; it measures and asserts nothing about the store.
//!
//! It opens a **clone** of a ledger and of its mirror namespace, and decides
//! every top-level `*.jsonl` in the given Claude Code folders exactly as a
//! reload does ([`LoadMirror::unchanged_how`]), twice in one process: the
//! first pass is as cold as the clone's page cache, the second is warm.
//!
//! ```text
//! THRULINE_PROBE_CLONE_DB=<scratch>/ledger.db \
//! THRULINE_PROBE_CLONE_MIRRORS=<scratch>/mirrors \
//! THRULINE_PROBE_CLAUDE_DIRS=<folder>[:<folder>…] \
//!   cargo test --release -p thruline-core --test hq_load_provenance_probe -- --ignored --nocapture
//! ```
//!
//! The founder's store is never opened: a ledger or mirror path inside
//! `~/.thruline` is refused before anything is opened (C29: an empty `-wal`
//! appearing beside the live ledger is already a write to it). Take the clone
//! with `cp -c` when no `ledger.db-wal` exists.

use std::path::{Path, PathBuf};
use std::time::Instant;

use thruline_core::ingest::{LoadMirror, Unchanged};
use thruline_core::Ledger;

fn refuse_the_live_store(path: &Path) {
    let home = std::env::var_os("HOME").map(PathBuf::from).expect("HOME");
    let live = home.join(".thruline");
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let live = std::fs::canonicalize(&live).unwrap_or(live);
    assert!(
        !canonical.starts_with(&live),
        "refused: {} is inside the live store {}; point the probe at a clone",
        path.display(),
        live.display()
    );
}

#[test]
#[ignore = "measurement against a clone named by THRULINE_PROBE_CLONE_DB"]
fn time_the_claude_decisions_on_a_clone() {
    let (Some(db), Some(mirrors), Some(dirs)) = (
        std::env::var_os("THRULINE_PROBE_CLONE_DB").map(PathBuf::from),
        std::env::var_os("THRULINE_PROBE_CLONE_MIRRORS").map(PathBuf::from),
        std::env::var("THRULINE_PROBE_CLAUDE_DIRS").ok(),
    ) else {
        eprintln!(
            "THRULINE_PROBE_CLONE_DB / _CLONE_MIRRORS / _CLAUDE_DIRS not set; nothing measured"
        );
        return;
    };
    refuse_the_live_store(&db);
    refuse_the_live_store(&mirrors);
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in dirs.split(':').filter(|d| !d.is_empty()) {
        let mut own: Vec<PathBuf> = std::fs::read_dir(dir)
            .expect("read folder")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "jsonl"))
            .collect();
        own.sort();
        files.extend(own);
    }
    let mut ledger = Ledger::open(&db).expect("open clone");
    let loader = LoadMirror::open(&mut ledger, &mirrors).expect("open mirror clone");
    for pass in ["cold", "warm"] {
        let t = Instant::now();
        let (mut by_stat, mut by_content, mut no) = (0, 0, 0);
        for f in &files {
            match loader.unchanged_how(&ledger, f, false) {
                Unchanged::ByStat => by_stat += 1,
                Unchanged::ByContent => by_content += 1,
                Unchanged::No => no += 1,
            }
        }
        eprintln!(
            "probe {pass}: {} Claude Code files decided in {:.3} s — by stat {by_stat}, by content {by_content}, full path {no}",
            files.len(),
            t.elapsed().as_secs_f64()
        );
    }
}
