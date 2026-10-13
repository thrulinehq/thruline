//! C27 (c): the server caches each project's serialized view keyed on the
//! index fingerprint and `Index::active_threads_at`. That is sound only while
//! the set of threads active at `now` is the ONLY thing in a project view
//! that depends on the clock. This pins it: at many instants — every thread's
//! active-window edges, both sides — two views with the same active set
//! serialize to the same bytes, and the set does change across the instants.
//! It also pins that the set agrees with `thread_active_at`, thread by thread.

mod scratch;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use scratch::TempTree;
use thruline_core::hq;
use thruline_core::ingest::{ingest, ingest_codex, CodexIngestOptions};
use thruline_core::Ledger;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

#[test]
fn a_project_view_depends_on_the_clock_only_through_active_threads_at() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(
        &mut ledger,
        &[fixtures().join("claude/-work-app")],
        &tmp.path().join("mirrors"),
    )
    .expect("claude ingest");
    let codex: Vec<PathBuf> = std::fs::read_dir(fixtures().join("codex"))
        .expect("codex fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    ingest_codex(
        &mut ledger,
        &codex,
        &CodexIngestOptions {
            mirror_root: tmp.path().join("mirrors"),
            owner_key: Some("hq-test-owner".to_string()),
        },
    )
    .expect("codex ingest");
    let project = hq::hq_projects(&ledger).expect("projects").remove(0);
    let index = hq::index(&ledger, &project).expect("index");
    let base = hq::parse_ms("2026-10-07T00:00:00Z").expect("instant");
    let threads = index.view(base).threads;
    assert!(threads.len() > 1, "the fixture needs several threads");

    // Every thread's window edges, on both sides, and far away.
    let mut instants = vec![base, i64::MIN, i64::MAX, 0];
    for thread in &threads {
        let last = thread
            .last_activity_at
            .as_deref()
            .and_then(hq::parse_ms)
            .expect("last activity");
        instants.extend([
            last - 86_400_000,
            last - hq::AGENT_ACTIVE_MS - 1,
            last - hq::AGENT_ACTIVE_MS,
            last - 60_000,
            last,
            last + 60_000,
            last + hq::AGENT_ACTIVE_MS - 1,
            last + hq::AGENT_ACTIVE_MS,
            last + hq::AGENT_ACTIVE_MS + 1,
            last + 86_400_000,
        ]);
    }

    let mut by_set: BTreeMap<Vec<String>, Vec<u8>> = BTreeMap::new();
    for now in instants {
        let set: Vec<String> = index
            .active_threads_at(now)
            .into_iter()
            .map(str::to_string)
            .collect();
        for thread in &threads {
            let active = index.thread_active_at(&thread.id, now).expect("thread");
            assert_eq!(
                active,
                set.contains(&thread.id),
                "active_threads_at and thread_active_at disagree on {} at {now}",
                thread.id
            );
        }
        let bytes = serde_json::to_vec(&index.view(now)).expect("json");
        match by_set.get(&set) {
            Some(seen) => assert_eq!(
                seen, &bytes,
                "the project view changed at {now} without active_threads_at changing"
            ),
            None => {
                by_set.insert(set, bytes);
            }
        }
    }
    assert!(
        by_set.len() > 2,
        "the active set took {} values: the test proved too little",
        by_set.len()
    );
    // And a different set is a different view: the key is not wider than it
    // needs to be for the cache to be right, but it must not be narrower.
    let views: std::collections::BTreeSet<&Vec<u8>> = by_set.values().collect();
    assert_eq!(views.len(), by_set.len(), "two active sets gave one view");
}
