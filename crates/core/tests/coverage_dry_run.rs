//! Live-project §5r diagnostic with no scoring or annotation writes.
//!
//! Opening an older ledger may apply the conservative §5r schema migration;
//! the measurement itself does not alter claims, searches, links, or verdicts.

use thruline_core::annotate::coverage_dry_run;
use thruline_core::config::Config;
use thruline_core::ledger::Ledger;

#[path = "support/live_store.rs"]
mod live_store;

#[test]
#[ignore = "requires the user's live ledger; measurement only"]
fn report_coverage_funnel_without_scoring_or_writing() {
    let project =
        std::env::var("THRULINE_COVERAGE_PROJECT").expect("THRULINE_COVERAGE_PROJECT is required");
    let db = std::env::var("THRULINE_LEDGER").expect("THRULINE_LEDGER is required");
    live_store::refuse_live_store(std::path::Path::new(&db));
    let ledger = Ledger::open(db).expect("open ledger");
    let r = coverage_dry_run(&ledger, &project, &Config::default()).expect("coverage dry-run");

    println!(
        "coverage-funnel claims {} -> eligible {} | would-be unsupported {} -> unknown {} / emitted {}",
        r.claims,
        r.coverage_eligible,
        r.would_be_unsupported,
        r.now_unknown,
        r.now_emitted
    );
    println!("project family profile");
    for (family, count) in r.project_family_profile {
        println!("  {family:<20} {count}");
    }
    println!("coverage misses by family");
    for (family, count) in r.missing_by_family {
        println!("  {family:<20} {count}");
    }
}
