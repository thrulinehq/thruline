//! L8 — THE WEDGE, END TO END. **Zero seeded events.**
//!
//! The whole product in one walk, with no asterisk anywhere:
//!
//!   Claude-side work (real JSONL ingest)
//!     → both agents' projects resolve to ONE repo (A92 identity)
//!     → packet #1 compiled and ISSUED to the Claude-side agent
//!     → `packet::handoff` freezes that exact issue into a document
//!     → codex-side continuation (real rollout ingest)
//!     → a REAL wrapper subprocess run, accepted as an assured pair
//!     → packet #2 issued as the SUCCESSOR of packet #1
//!     → and #2's VERIFIED section renders the claim line that #1 could not.
//!
//! **The walk itself lives in `thruline_core::demo`, not here** (A122). The
//! shipped `thruline demo` subcommand runs that exact function, so the demo a
//! stranger watches and the walk this file pins are one code path. A demo that
//! drifted from its own test would be a demo of nothing.
//!
//! What stays here is the part a test is for: the assertions.

use tempfile::TempDir;
use thruline_core::demo;

fn walked() -> (TempDir, demo::DemoReport) {
    let tmp = TempDir::new().expect("tempdir");
    let report = demo::walk(tmp.path()).expect("the walk must complete");
    (tmp, report)
}

/// THE ACCEPT. Packet #1 cannot render a verified claim; packet #2 can; the
/// only thing that happened in between is a command that actually ran.
#[test]
fn the_wedge_walks_end_to_end_with_zero_seeded_events() {
    let (_tmp, r) = walked();

    // Two agents, two project ids, one repo. Nothing is passed between them —
    // they meet because both recorded cwds resolve to the same git root.
    assert_eq!(
        r.project_ids.len(),
        2,
        "two agents in one repo are still two projects: {:?}",
        r.project_ids
    );
    assert!(!r.repo_id.is_empty(), "the join is the repo");

    // Packet #1: a MEASURED zero. Not silence, and not borrowed from
    // `supported` to look fuller than it is.
    assert!(
        r.first_verified_section
            .contains("are verified and current"),
        "packet #1 must report a measured zero: {}",
        r.first_verified_section
    );
    assert!(
        !r.first_verified_section.contains("evidence=verified"),
        "and there is genuinely nothing to show yet: {}",
        r.first_verified_section
    );

    // THE PAYOFF: the continuation renders the verified claim. Before A115 no
    // packet could — the read was node-level and RULING 4 caps nodes at
    // `supported`.
    assert!(
        r.second_verified_section.contains("evidence=verified"),
        "packet #2 must render the verified claim: {}",
        r.second_verified_section
    );
    assert!(
        r.second_verified_section.contains("currency=current"),
        "with its currency axis: {}",
        r.second_verified_section
    );
    assert!(
        r.second_verified_section.contains("SHADOW"),
        "and its §5s disclosure: {}",
        r.second_verified_section
    );

    // The chain: #1 starts it, #2 names #1 as its predecessor.
    assert!(
        r.frozen_handoff.contains(&r.first_issue)
            && r.frozen_handoff.contains("this is the start of the chain"),
        "the frozen document names itself and its position: {}",
        r.frozen_handoff
    );
    assert!(
        r.frozen_successor
            .contains(&format!("continues from issue {}", r.first_issue)),
        "and the successor records where the work came from: {}",
        r.frozen_successor
    );
    assert_ne!(r.first_issue, r.second_issue, "two distinct issues");

    // The negative that makes the walk mean something. If a single event had
    // been seeded, the verdict above would be an artefact of the fixture.
    assert_eq!(
        r.seeded_events, 0,
        "zero seeded events, or the walk proves nothing"
    );
}

/// The walk is deterministic in what it PROVES, though not in its ids: two runs
/// produce different ulids and different repo hashes, and must still both reach
/// a verified claim. A demo that only worked on a lucky run is not a demo.
#[test]
fn the_walk_reaches_the_same_verdict_on_an_independent_run() {
    let (_a, first) = walked();
    let (_b, second) = walked();

    assert_ne!(
        first.first_issue, second.first_issue,
        "independent runs, independent issues"
    );
    for r in [&first, &second] {
        assert!(
            r.second_verified_section.contains("evidence=verified"),
            "every run must reach the verified claim"
        );
        assert_eq!(r.seeded_events, 0);
    }
}
