//! Property tests for the §3.1 hash chain.
//!
//! The unit tests in `spec::chain` check named scenarios. These check the
//! *claim* the chain is made of: **no edit to any field of any event survives
//! verification.** A scenario test can only ever show that the edits someone
//! thought of are caught; this generates the chains and the edits.
//!
//! It also pins the chain's one real limitation — see
//! `a_prefix_of_a_valid_chain_is_itself_valid`. That property is not a defect
//! to be fixed here, but it is a fact a receipt has to carry (the head hash and
//! the event count together are what make truncation visible), and an honest
//! suite states it rather than leaving a reader to assume otherwise.

use proptest::prelude::*;
use serde_json::json;

use thruline_core::spec::chain::{append, genesis_prev_hash, verify_chain};
use thruline_core::spec::event::{
    Event, EventId, EventKind, EventWire, OpenPayload, Payload, Source,
};

/// The timestamps a generated event may carry. All valid RFC 3339 UTC; the
/// point of varying them is that `ts` is inside the hash preimage.
const TIMESTAMPS: [&str; 4] = [
    "2026-09-02T10:11:12Z",
    "2026-09-02T10:11:12.345Z",
    "2024-02-29T23:59:60Z",
    "1970-01-01T00:00:00.000000001Z",
];

fn source_strategy() -> impl Strategy<Value = Source> {
    prop_oneof![
        Just(Source::ClaudeCodeHook),
        Just(Source::CodexHook),
        Just(Source::GeminiHook),
        Just(Source::CodexSessionFile),
        Just(Source::ExecRecorder),
        Just(Source::FsWatcher),
        Just(Source::Git),
        Just(Source::Thruline),
        "[a-z-]{1,12}".prop_map(Source::Other),
    ]
}

/// (seq gap, timestamp index, note text, source) for one event.
type Spec = (u64, usize, String, Source);

fn chain_strategy() -> impl Strategy<Value = (String, Vec<Spec>)> {
    (
        "[a-zA-Z0-9_-]{1,24}",
        prop::collection::vec(
            (
                1u64..5,
                0usize..TIMESTAMPS.len(),
                ".{0,40}",
                source_strategy(),
            ),
            1..7,
        ),
    )
}

fn build(session_id: &str, specs: &[Spec]) -> Vec<Event> {
    let mut chain: Vec<Event> = Vec::new();
    let mut seq = 0u64;
    for (gap, ts_index, text, source) in specs {
        seq += gap;
        let mut payload = OpenPayload::new();
        payload.insert("text".into(), json!(text));
        payload.insert("n".into(), json!(seq));
        let event = Event::new(
            session_id,
            seq,
            TIMESTAMPS[*ts_index],
            source.clone(),
            Payload::Note(payload),
            "placeholder",
        )
        .expect("timestamps in TIMESTAMPS are valid");
        append(&mut chain, event).expect("appending to a sealed chain cannot fail");
    }
    chain
}

/// Every mutation below is guaranteed to change the event; each returns `false`
/// if it could not (so the property can skip rather than pass vacuously).
const FIELDS: usize = 10;

/// Mutations operate on [`EventWire`], because `Event`'s fields are private —
/// deliberately, since public fields were the F1 bypass that let an unhashable
/// integer into the store without ever meeting the gate.
///
/// A mutation the gate REFUSES is itself a detection, and the caller treats it
/// as one; a mutation it accepts must then break the chain.
fn mutate(wire: &mut EventWire, field: usize) -> bool {
    match field {
        0 => {
            wire.v = wire.v.wrapping_add(1);
            true
        }
        1 => {
            let before = wire.id.clone();
            wire.id = EventId::generate();
            wire.id != before
        }
        2 => {
            wire.session_id.push('x');
            true
        }
        3 => {
            wire.seq = wire.seq.wrapping_add(1);
            true
        }
        4 => {
            let before = wire.ts.clone();
            wire.ts = if before == TIMESTAMPS[0] {
                TIMESTAMPS[1].to_string()
            } else {
                TIMESTAMPS[0].to_string()
            };
            wire.ts != before
        }
        5 => {
            let before = wire.source.clone();
            wire.source = if before == Source::Git {
                Source::Thruline
            } else {
                Source::Git
            };
            wire.source != before
        }
        6 => {
            let before = wire.kind.clone();
            wire.kind = if before == EventKind::Note {
                EventKind::Exec
            } else {
                EventKind::Note
            };
            wire.kind != before
        }
        7 => {
            // Adding a key rather than changing one, because adding is the edit
            // a reader is least likely to notice.
            if let serde_json::Value::Object(map) = &mut wire.payload {
                map.insert("injected".into(), json!("x"));
                true
            } else {
                false
            }
        }
        8 => {
            wire.prev_hash.push_str("00");
            true
        }
        9 => {
            // The hash itself: the one field outside the preimage, and the one
            // an attacker would edit to make a tampered event self-consistent.
            match wire.hash.take() {
                Some(hash) => {
                    wire.hash = Some(format!("{hash}ff"));
                    true
                }
                None => false,
            }
        }
        _ => false,
    }
}

/// Apply `field`'s mutation to `event`.
///
/// `Ok(None)` means the gate refused the mutated wire — a detection.
/// `Ok(Some(e))` is a tampered event the chain must then reject.
/// `Err(())` means the mutation was a no-op and the case should be skipped.
fn tamper(event: &Event, field: usize) -> Result<Option<Event>, ()> {
    // `into_wire` drops the hash, so the ORIGINAL is put back deliberately:
    // a tampering fixture is exactly an event whose carried hash no longer
    // matches its contents, and that has to be spelled out rather than
    // inherited. `Event::edit_and_seal` cannot express it, by design.
    let carried = event.hash().map(str::to_string);
    let mut wire = event.clone().into_wire();
    wire.hash = carried;
    if !mutate(&mut wire, field) {
        return Err(());
    }
    Ok(Event::try_from(wire).ok())
}

fn field_name(field: usize) -> &'static str {
    [
        "v",
        "id",
        "session_id",
        "seq",
        "ts",
        "source",
        "kind",
        "payload",
        "prev_hash",
        "hash",
    ]
    .get(field)
    .copied()
    .unwrap_or("?")
}

/// Exhaustive and deterministic: every field, at every position, in one chain.
///
/// The property test below samples `field`, so a mutator that had quietly
/// become a no-op would show up only as proptest rejections — which do not
/// fail a run. This closes that: each of the ten mutators must both *apply*
/// and *break the chain*, at each of three positions, every time the suite
/// runs.
#[test]
fn every_mutator_applies_and_breaks_at_every_position() {
    let specs: Vec<Spec> = (0..3)
        .map(|i| (1u64, i % TIMESTAMPS.len(), format!("note {i}"), Source::Git))
        .collect();
    let chain = build("sess-fixed", &specs);
    assert!(
        verify_chain(&chain).is_ok(),
        "the fixture chain must verify"
    );

    for field in 0..FIELDS {
        for index in 0..chain.len() {
            let mutated = tamper(&chain[index], field).unwrap_or_else(|()| {
                panic!(
                    "mutator for `{}` reported no-op at index {index} — it has \
                     become dead, and the property test would only have skipped it",
                    field_name(field)
                )
            });
            let Some(tampered_event) = mutated else {
                // The gate refused it. That IS the detection.
                continue;
            };
            assert_ne!(
                tampered_event,
                chain[index],
                "mutator for `{}` at index {index} changed nothing",
                field_name(field)
            );
            let mut tampered = chain.clone();
            tampered[index] = tampered_event;
            assert!(
                verify_chain(&tampered).is_err(),
                "editing `{}` of event {index} did not break the chain",
                field_name(field)
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The chain a generator builds always verifies, and its head is the last
    /// event's hash. Without this, every refutation below could be passing
    /// because the generator produces garbage.
    #[test]
    fn generated_chains_verify((session_id, specs) in chain_strategy()) {
        let chain = build(&session_id, &specs);
        let head = verify_chain(&chain).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(head.event_count, chain.len() as u64);
        prop_assert_eq!(head.session_id, session_id.clone());
        prop_assert_eq!(Some(head.head_hash.as_str()), chain[chain.len() - 1].hash());
        prop_assert_eq!(chain[0].prev_hash(), genesis_prev_hash(&session_id));
    }

    /// **The property.** Mutate any field of any event and the chain stops
    /// verifying.
    #[test]
    fn any_mutation_of_any_field_breaks_the_chain(
        (session_id, specs) in chain_strategy(),
        index in 0usize..8,
        field in 0usize..FIELDS,
    ) {
        let chain = build(&session_id, &specs);
        let index = index % chain.len();

        let Ok(mutated) = tamper(&chain[index], field) else {
            // The mutation could not produce a different value. Skipping is
            // correct; passing would be a vacuous assertion.
            return Err(TestCaseError::reject("mutation was a no-op"));
        };
        let Some(tampered_event) = mutated else {
            // The construction gate refused it outright, which is a detection.
            return Ok(());
        };
        prop_assert_ne!(&tampered_event, &chain[index],
            "mutator for {} did not change the event", field_name(field));

        let mut tampered = chain.clone();
        tampered[index] = tampered_event;
        prop_assert!(
            verify_chain(&tampered).is_err(),
            "editing `{}` of event {} of {} did not break the chain",
            field_name(field), index, chain.len()
        );
    }

    /// Deleting an event is caught wherever it happens — except at the very
    /// end, which is the next property.
    #[test]
    fn deleting_any_interior_event_breaks_the_chain(
        (session_id, specs) in chain_strategy().prop_filter(
            "need at least two events", |(_, s)| s.len() >= 2
        ),
        index in 0usize..8,
    ) {
        let chain = build(&session_id, &specs);
        let index = index % (chain.len() - 1); // never the last
        let mut short = chain.clone();
        short.remove(index);
        prop_assert!(
            verify_chain(&short).is_err(),
            "removing event {} of {} left a chain that still verifies",
            index, chain.len()
        );
    }

    /// Inserting a foreign event anywhere breaks the chain.
    #[test]
    fn inserting_a_foreign_event_breaks_the_chain(
        (session_id, specs) in chain_strategy(),
        at in 0usize..8,
    ) {
        let chain = build(&session_id, &specs);
        let at = at % (chain.len() + 1);
        let forged = Event::new(
            &session_id,
            0,
            TIMESTAMPS[0],
            Source::Thruline,
            Payload::Note(OpenPayload::new()),
            genesis_prev_hash(&session_id),
        )
        .expect("constructible")
        .seal()
        .expect("seals");
        let mut spliced = chain.clone();
        spliced.insert(at, forged);
        prop_assert!(
            verify_chain(&spliced).is_err(),
            "splicing a forged event in at {} of {} still verified", at, chain.len()
        );
    }

    /// **The limitation, stated as a property.** A hash chain links each event
    /// to the one before it, so dropping events from the *end* leaves a chain
    /// that is internally perfect — it just ends earlier.
    ///
    /// This is why §8.1's receipt carries `event_chain_head` *and*
    /// `event_count`: the head alone cannot tell a complete session from a
    /// truncated one. Recorded here so no reader concludes from
    /// `deleting_any_interior_event_breaks_the_chain` that deletion in general
    /// is caught.
    #[test]
    fn a_prefix_of_a_valid_chain_is_itself_valid((session_id, specs) in chain_strategy()) {
        let chain = build(&session_id, &specs);
        for keep in 1..=chain.len() {
            let prefix = &chain[..keep];
            let head = verify_chain(prefix)
                .map_err(|e| TestCaseError::fail(format!("prefix of {keep}: {e}")))?;
            prop_assert_eq!(head.event_count, keep as u64);
        }
    }
}
