//! One test, alone in its own binary, because it measures process memory.
//!
//! Resident set size is a property of the *process*, so a sibling test running
//! in parallel would land in the measurement. Cargo runs test binaries as
//! separate processes, so isolating the file isolates the number.
//!
//! # Why this test exists
//!
//! I previously claimed the ledger read was bounded "by construction" and was
//! correctly told that construction is not proof. `verify_session` used to
//! materialise every event of a session into a `Vec` — on a two-million-event
//! imported session, unbounded memory, in the one code path whose job is to
//! tell you whether the record is intact.
//!
//! This measures the difference rather than asserting it. The assertion is a
//! *ratio* between the two readers, not an absolute megabyte figure: absolute
//! RSS depends on the allocator, the page size and what the harness already
//! touched, and a threshold tuned on this machine would be a flake generator on
//! any other. The ratio is a property of the algorithms.

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use thruline_core::spec::chain::append;
use thruline_core::spec::event::{Event, OpenPayload, Payload, Source};
use thruline_ledger::{EventRow, Ledger};

/// Events in the fixture session. Large enough that a `Vec<Event>` is clearly
/// visible in RSS, small enough that the suite stays quick.
const EVENTS: u64 = 60_000;

fn rss_bytes(system: &mut System, pid: Pid) -> u64 {
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    system.process(pid).map(|p| p.memory()).unwrap_or(0)
}

#[test]
// **Ignored because its instrument is measured-dead, not because it is
// inconvenient (D29).** It fails about 1 run in 10 while the property it claims
// to prove holds on every run — so a red here has never been evidence of a
// regression, and a green has never been evidence of the property.
//
// D29 measured why: an RSS *delta* is heap GROWTH, a fact about what ran
// before, not about the reader. A collecting read of 60,000 events cost ZERO
// new resident bytes once the heap had grown. The shipped test only ever
// worked because the first collect happened to be the thing that grew the
// heap. Two replacement instruments were worse (peak-above-baseline failed 4
// of 8 with the baseline already containing the high-water mark; one-reader-
// per-child failed 1 of 8 with the spike surviving process isolation), and the
// remaining hypothesis — SQLite growing its own page cache during the scan —
// is unverified. The load hypothesis is separately FALSIFIED, by p3's batch C;
// do not reintroduce it.
//
// WHAT IS NO LONGER CHECKED ON EVERY GATE: that `verify_session` is O(1) in
// session size rather than O(n) — that it streams instead of materialising.
// The property is believed to hold and has never been observed to fail; what
// failed is every attempt to measure it. Run it deliberately with
// `cargo test -p thruline-ledger --test streaming_memory -- --ignored`, in
// which form it passed 3 of 3 immediately before this was applied.
//
// The ignore comes off when an instrument exists that measures the reader
// rather than the heap's history. Until then this is a measurement taken when
// someone asks, not a gate.
#[ignore = "D29: the RSS-delta instrument measures heap growth, not reader cost; \
            fails ~1 run in 10 while the property holds. Run with --ignored."]
fn verifying_a_session_does_not_hold_the_session_in_memory() {
    let ledger = Ledger::open_in_memory().expect("open");
    let repo = ledger.repo("mem-test", None).expect("repo");
    ledger
        .upsert_session("big", &repo, "claude-code", None, false, None)
        .expect("session");

    // Build and store the chain in batches, so the fixture itself does not
    // dominate the measurement.
    let mut prev: Option<String> = None;
    let mut seq = 0u64;
    while seq < EVENTS {
        let mut batch: Vec<Event> = Vec::new();
        for _ in 0..5_000 {
            seq += 1;
            let mut payload = OpenPayload::new();
            payload.insert("i".into(), serde_json::json!(seq));
            payload.insert("filler".into(), serde_json::json!("x".repeat(200)));
            let event = Event::new(
                "big",
                seq,
                "2026-09-02T10:11:12.345Z",
                Source::ExecRecorder,
                Payload::Note(payload),
                prev.clone().unwrap_or_default(),
            )
            .expect("constructible");
            append(&mut batch, event).expect("appends");
        }
        // `append` re-roots each batch at genesis, so only the first batch is
        // linked correctly that way; re-link the rest onto the running head.
        if let Some(head) = &prev {
            let mut relinked: Vec<Event> = Vec::new();
            for event in batch.drain(..) {
                let prev = relinked
                    .last()
                    .and_then(|e: &Event| e.hash().map(str::to_string))
                    .unwrap_or_else(|| head.clone());
                relinked.push(event.with_prev_hash(prev).seal().expect("seals"));
            }
            batch = relinked;
        }
        prev = batch.last().and_then(|e| e.hash().map(str::to_string));
        let rows: Vec<EventRow> = batch.into_iter().map(EventRow::bare).collect();
        ledger.append_events(&repo, &rows).expect("append");
    }

    let head = ledger
        .verify_session("big")
        .expect("the fixture chain verifies");
    assert_eq!(
        head.event_count, EVENTS,
        "the fixture is not the size claimed"
    );

    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();

    // Streaming first: the collecting reader's allocations will not be returned
    // to the OS afterwards, so measuring it first would poison the baseline.
    let before_stream = rss_bytes(&mut system, pid);
    let streamed = ledger.verify_session("big").expect("verifies");
    let after_stream = rss_bytes(&mut system, pid);
    let stream_delta = after_stream.saturating_sub(before_stream);

    let before_collect = rss_bytes(&mut system, pid);
    let collected = ledger.session_events("big").expect("read");
    let after_collect = rss_bytes(&mut system, pid);
    let collect_delta = after_collect.saturating_sub(before_collect);

    assert_eq!(collected.len() as u64, EVENTS);
    assert_eq!(streamed.head_hash, head.head_hash);

    println!(
        "EVENTS={EVENTS}  streaming_rss_delta={stream_delta} bytes  \
         collecting_rss_delta={collect_delta} bytes"
    );

    // The collecting read must be visibly expensive, or the comparison below is
    // measuring nothing at all. 60,000 events with a 200-byte filler each is at
    // least a few megabytes of live data.
    assert!(
        collect_delta > 4 * 1024 * 1024,
        "collecting {EVENTS} events grew RSS by only {collect_delta} bytes — the \
         probe is not measuring anything, so the comparison proves nothing"
    );

    // And the streaming read must be a small fraction of it. Ten percent is a
    // deliberately loose bound: the claim is O(1) versus O(n), and anything
    // near parity means the streaming path is still materialising the session.
    assert!(
        stream_delta * 10 < collect_delta,
        "verify_session grew RSS by {stream_delta} bytes against {collect_delta} \
         for the collecting read — it is still holding the session in memory"
    );
}
