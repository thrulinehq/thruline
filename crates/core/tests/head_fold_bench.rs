//! L9.6 — the chain-position fold, measured on the live ledger.
//!
//! A97 named this debt and left it: the packet's binding folds every event in a
//! project to derive its chain position, so the cost scales with project size
//! rather than with session count. On the largest project that was ~1.2 s, which is the
//! whole read budget spent before a single line is compiled.
//!
//! This is a MEASUREMENT, not a unit test. It reads the developer's real ledger
//! read-only and skips cleanly when there isn't one, so CI never depends on a
//! corpus that only exists on one machine.
//!
//!   cargo test --release --test head_fold_bench -- --nocapture --ignored

use std::time::Instant;

use thruline_core::Ledger;

#[path = "support/live_store.rs"]
mod live_store;

fn live_ledger() -> Option<Ledger> {
    // THRULINE_BENCH_LEDGER lets the after-measurement run against a CLONE, so
    // the developer's real ledger is never written to just to produce a number.
    let path = match std::env::var("THRULINE_BENCH_LEDGER") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => std::path::PathBuf::from(std::env::var("HOME").ok()?).join(".thruline/ledger.db"),
    };
    if !path.exists() {
        return None;
    }
    live_store::refuse_live_store(&path);
    Ledger::open(&path).ok()
}

#[test]
#[ignore = "reads the developer's live ledger; run explicitly with --ignored"]
fn the_chain_position_fold_costs_this_much_per_project() {
    let Some(ledger) = live_ledger() else {
        println!("SKIP — no ~/.thruline/ledger.db on this machine");
        return;
    };

    let mut projects: Vec<(String, i64)> = ledger
        .conn()
        .prepare(
            "SELECT project_id, COUNT(*) FROM events GROUP BY project_id ORDER BY COUNT(*) DESC",
        )
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                .and_then(|rows| rows.collect())
        })
        .expect("project sizes");
    projects.truncate(8);

    let total_events: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("total");
    let total_sessions: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .expect("sessions");
    println!("\n=== CHAIN-POSITION FOLD — live ledger ===");
    println!("ledger: {total_events} events across {total_sessions} sessions\n");
    println!(
        "{:<46} {:>10} {:>9} {:>12}  path",
        "project", "events", "sessions", "head read"
    );

    for (project_id, events) in &projects {
        let sessions: i64 = ledger
            .conn()
            .query_row(
                "SELECT COUNT(DISTINCT session_id) FROM events WHERE project_id = ?1",
                [project_id],
                |r| r.get(0),
            )
            .expect("sessions");
        // Warm the page cache first, so the number reported is the fold's cost
        // and not the first read's disk cost.
        let (_, path) = ledger.project_head_with_path(project_id).expect("warm");
        let mut best = f64::MAX;
        for _ in 0..25 {
            let t = Instant::now();
            let _ = ledger.compute_project_head(project_id).expect("head");
            best = best.min(t.elapsed().as_secs_f64() * 1_000_000.0);
        }
        // The cache is only worth reporting if it is RIGHT. Every project is
        // checked against the uncached fold before its number is printed.
        let truth = ledger
            .fold_project_head_from_events(project_id)
            .expect("fold");
        let served = ledger.compute_project_head(project_id).expect("served");
        assert_eq!(
            served, truth,
            "cache disagrees with the fold for {project_id}"
        );
        let short: String = project_id.chars().take(44).collect();
        println!(
            "{short:<46} {events:>10} {sessions:>9} {best:>9.1} us  {}",
            path.as_str()
        );
    }
    println!("\n(best of 25, warm; every number above verified equal to the uncached fold)\n");
}

/// L9.7 — where the packet's time actually goes, phase by phase.
///
/// The end-to-end CLI number includes two `Ledger::open` calls and process
/// start. This isolates the compile itself, so a budget failure can be
/// attributed to a phase instead of to "the packet".
#[test]
#[ignore = "reads the developer's live ledger; run explicitly with --ignored"]
fn the_packet_compile_costs_this_much_per_phase() {
    let Some(ledger) = live_ledger() else {
        println!("SKIP — no ~/.thruline/ledger.db on this machine");
        return;
    };
    let mut projects: Vec<(String, i64)> = ledger
        .conn()
        .prepare(
            "SELECT project_id, COUNT(*) FROM events GROUP BY project_id ORDER BY COUNT(*) DESC",
        )
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                .and_then(|rows| rows.collect())
        })
        .expect("sizes");
    projects.truncate(5);

    println!("\n=== PACKET COMPILE, BY PHASE ===");
    println!(
        "{:<44} {:>9} {:>7} {:>9} {:>9} {:>9}",
        "project", "events", "nodes", "head", "view", "compile"
    );
    for (pid, events) in &projects {
        let nodes: i64 = ledger
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE project_id = ?1",
                [pid],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let scope = thruline_core::packet::Scope::Project(pid.clone());
        let cfg = thruline_core::config::Config::default();
        let budget = thruline_core::packet::Budget::default();
        // warm
        let _ = thruline_core::packet::compile_request(&ledger, &scope, None, &budget, &cfg);

        let mut t_head = f64::MAX;
        let mut t_view = f64::MAX;
        let mut t_all = f64::MAX;
        for _ in 0..5 {
            let a = Instant::now();
            let (head, _) = ledger.compute_project_head(pid).expect("head");
            t_head = t_head.min(a.elapsed().as_secs_f64() * 1000.0);

            let b = Instant::now();
            let _ =
                thruline_core::packet::view_for_with_head(&ledger, pid, &head, &cfg).expect("view");
            t_view = t_view.min(b.elapsed().as_secs_f64() * 1000.0);

            let c = Instant::now();
            let _ = thruline_core::packet::compile_request(&ledger, &scope, None, &budget, &cfg)
                .expect("compile");
            t_all = t_all.min(c.elapsed().as_secs_f64() * 1000.0);
        }
        let short: String = pid.chars().take(42).collect();
        println!(
            "{short:<44} {events:>9} {nodes:>7} {t_head:>7.2}ms {t_view:>7.2}ms {t_all:>7.2}ms"
        );
    }
    println!("\n(best of 5, warm; 'compile' is the whole compile_request)\n");
}
