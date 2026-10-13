//! C27 (c): the server caches each thread's serialized view keyed on the
//! index fingerprint and `Index::thread_active_at`. That is sound only while
//! `thread_active_at` is the ONLY thing in a thread view that depends on the
//! clock. This pins it: at many instants, two views with the same answer
//! serialize to the same bytes, and the answer does flip across the window.

mod scratch;

use std::path::{Path, PathBuf};

use scratch::TempTree;
use thruline_core::hq;
use thruline_core::ingest::{ingest, ingest_codex, CodexIngestOptions};
use thruline_core::Ledger;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

#[test]
fn a_thread_view_depends_on_the_clock_only_through_thread_active_at() {
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
    let mut flipped = 0;
    for thread in index.view(base).threads {
        let last = thread
            .last_activity_at
            .as_deref()
            .and_then(hq::parse_ms)
            .expect("last activity");
        // Instants before, inside and after the active window, and far away.
        let instants = [
            last - 3_600_000,
            last,
            last + 60_000,
            last + hq::AGENT_ACTIVE_MS - 1,
            last + hq::AGENT_ACTIVE_MS + 1,
            last + 86_400_000,
            base,
        ];
        let mut by_answer: std::collections::BTreeMap<bool, Vec<u8>> = Default::default();
        for now in instants {
            let active = index.thread_active_at(&thread.id, now).expect("thread");
            let bytes = serde_json::to_vec(
                &index
                    .thread_view(&ledger, &thread.id, now)
                    .expect("view")
                    .expect("present"),
            )
            .expect("json");
            match by_answer.get(&active) {
                Some(seen) => assert_eq!(
                    seen, &bytes,
                    "thread {} changed at {now} without thread_active_at changing",
                    thread.id
                ),
                None => {
                    by_answer.insert(active, bytes);
                }
            }
        }
        if by_answer.len() == 2 {
            flipped += 1;
        }
    }
    assert!(
        flipped > 0,
        "no thread's answer flipped: the test proved nothing"
    );
}
