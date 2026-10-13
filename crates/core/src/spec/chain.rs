//! The §3.1 hash chain over a session's events.
//!
//! Each event's `hash` covers every one of its own fields, and its `prev_hash`
//! is the previous event's `hash`. Two consequences, and they are the whole
//! point:
//!
//! - Editing any field of any event breaks that event's own hash.
//! - Removing, reordering or inserting an event breaks the link at that
//!   position and at every position after it.
//!
//! So a chain that verifies to a known head hash is a statement about the
//! entire session, not about the events someone chose to show you. The head
//! goes into the receipt (§8.1 `session.event_chain_head`), which is what makes
//! the receipt a claim about the *record* rather than about a selection from it.
//!
//! # What this does not defend against
//!
//! A chain is only as good as the earliest hash a verifier already trusts. An
//! attacker who controls the whole file can rewrite every event and re-chain
//! from genesis, and the result verifies. That is why [`genesis_prev_hash`] is
//! derived from the session id rather than being a constant — it at least binds
//! a rewritten chain to the session it claims to be — and why the head hash is
//! published in a signed receipt, where a rewrite is detectable. Detecting an
//! offline rewrite of an unpublished ledger is not a property of hash chains
//! and is not claimed here.

use super::canonical;
use super::event::{Event, EventError};

/// A chain did not verify.
#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    /// An event's own hash does not match its contents.
    #[error("event at seq {seq} (index {index}) failed its own hash check")]
    Event {
        index: usize,
        seq: u64,
        #[source]
        source: EventError,
    },
    /// An event's `prev_hash` is not the previous event's `hash`.
    #[error(
        "chain break at index {index} (seq {seq}): prev_hash is {found} but the \
         preceding event hashes to {expected}"
    )]
    Broken {
        index: usize,
        seq: u64,
        expected: String,
        found: String,
    },
    /// Events from more than one session were handed in as one chain.
    #[error("event at index {index} belongs to session {found:?}, not {expected:?}")]
    ForeignSession {
        index: usize,
        expected: String,
        found: String,
    },
    /// `seq` did not increase. Equal or decreasing `seq` means the input is
    /// reordered or duplicated, which the link check alone would also catch —
    /// but reported here it names the actual fault.
    #[error("event at index {index} has seq {seq}, which does not follow {previous}")]
    OutOfOrder {
        index: usize,
        seq: u64,
        previous: u64,
    },
    /// Nothing to verify.
    #[error("cannot verify an empty chain")]
    Empty,
}

/// The result of a successful walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHead {
    /// The session every event belonged to.
    pub session_id: String,
    /// The last event's `hash` — the value that goes in a receipt.
    pub head_hash: String,
    /// How many events were proven.
    pub event_count: u64,
    /// The `seq` of the last event.
    pub last_seq: u64,
}

/// The `prev_hash` of a session's first event.
///
/// `sha256:` over the session id. Not a constant: a constant genesis would let
/// a whole chain be lifted from one session into another without breaking a
/// single link.
pub fn genesis_prev_hash(session_id: &str) -> String {
    canonical::digest_of_bytes(session_id.as_bytes())
}

/// A chain verifier that consumes events one at a time.
///
/// [`verify_chain`] needs the whole session in a slice. That is fine for a
/// session and wrong for a store: an imported session read back through
/// `SELECT` would be materialised entirely in memory before a single link was
/// checked. This walks the same rules in constant space, so the ledger can
/// verify a chain of any length while holding one event.
///
/// The two are held to the same behaviour by
/// `the_streaming_walker_and_the_slice_verifier_agree`.
#[derive(Debug)]
pub struct ChainWalker {
    session_id: String,
    expected_prev: String,
    previous_seq: Option<u64>,
    count: u64,
    index: usize,
}

impl ChainWalker {
    /// Start a walk for `session_id`, expecting its genesis link first.
    pub fn new(session_id: impl Into<String>) -> ChainWalker {
        let session_id = session_id.into();
        ChainWalker {
            expected_prev: genesis_prev_hash(&session_id),
            session_id,
            previous_seq: None,
            count: 0,
            index: 0,
        }
    }

    /// Feed the next event. Errors are the same ones [`verify_chain`] returns.
    pub fn push(&mut self, event: &Event) -> Result<(), ChainError> {
        let index = self.index;
        if event.session_id() != self.session_id {
            return Err(ChainError::ForeignSession {
                index,
                expected: self.session_id.clone(),
                found: event.session_id().to_string(),
            });
        }
        if let Some(previous) = self.previous_seq {
            if event.seq() <= previous {
                return Err(ChainError::OutOfOrder {
                    index,
                    seq: event.seq(),
                    previous,
                });
            }
        }
        if event.prev_hash() != self.expected_prev {
            return Err(ChainError::Broken {
                index,
                seq: event.seq(),
                expected: self.expected_prev.clone(),
                found: event.prev_hash().to_string(),
            });
        }
        event.verify_hash().map_err(|source| ChainError::Event {
            index,
            seq: event.seq(),
            source,
        })?;
        self.expected_prev = event.hash().unwrap_or_default().to_string();
        self.previous_seq = Some(event.seq());
        self.count += 1;
        self.index += 1;
        Ok(())
    }

    /// Finish, returning the head. Refuses an empty walk, as [`verify_chain`]
    /// refuses an empty slice — a vacuous "verified" is the worst answer here.
    pub fn finish(self) -> Result<ChainHead, ChainError> {
        if self.count == 0 {
            return Err(ChainError::Empty);
        }
        Ok(ChainHead {
            head_hash: self.expected_prev,
            event_count: self.count,
            last_seq: self.previous_seq.unwrap_or(0),
            session_id: self.session_id,
        })
    }
}

/// Walk a session's events in order and prove the chain, returning the head.
///
/// `events` must be the session's events sorted by `seq` ascending. Every
/// event must be sealed.
pub fn verify_chain(events: &[Event]) -> Result<ChainHead, ChainError> {
    let Some(first) = events.first() else {
        return Err(ChainError::Empty);
    };
    let session_id = first.session_id().to_string();
    let mut expected_prev = genesis_prev_hash(&session_id);
    let mut previous_seq: Option<u64> = None;

    for (index, event) in events.iter().enumerate() {
        if event.session_id() != session_id {
            return Err(ChainError::ForeignSession {
                index,
                expected: session_id,
                found: event.session_id().to_string(),
            });
        }
        if let Some(previous) = previous_seq {
            if event.seq() <= previous {
                return Err(ChainError::OutOfOrder {
                    index,
                    seq: event.seq(),
                    previous,
                });
            }
        }
        if event.prev_hash() != expected_prev {
            return Err(ChainError::Broken {
                index,
                seq: event.seq(),
                expected: expected_prev,
                found: event.prev_hash().to_string(),
            });
        }
        event.verify_hash().map_err(|source| ChainError::Event {
            index,
            seq: event.seq(),
            source,
        })?;
        // `verify_hash` succeeded, so the carried hash is the computed one.
        expected_prev = event.hash().unwrap_or_default().to_string();
        previous_seq = Some(event.seq());
    }

    Ok(ChainHead {
        head_hash: expected_prev,
        event_count: events.len() as u64,
        last_seq: previous_seq.unwrap_or(0),
        session_id,
    })
}

/// Seal `event` onto the end of `chain`, setting its `prev_hash` for you.
///
/// The only way to extend a chain without hand-copying a hash, which is where
/// chains get broken in practice.
pub fn append(chain: &mut Vec<Event>, event: Event) -> Result<(), ChainError> {
    let prev = match chain.last() {
        Some(last) => last
            .hash()
            .ok_or_else(|| ChainError::Event {
                index: chain.len() - 1,
                seq: last.seq(),
                source: EventError::Unsealed(last.id().to_string()),
            })?
            .to_string(),
        None => genesis_prev_hash(event.session_id()),
    };
    let seq = event.seq();
    let sealed = event
        .with_prev_hash(prev)
        .seal()
        .map_err(|source| ChainError::Event {
            index: chain.len(),
            seq,
            source,
        })?;
    chain.push(sealed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::event::{EventWire, OpenPayload, Payload, Source};

    /// Build a tampered event: edit through the wire, then put the ORIGINAL
    /// hash back, so the result asserts a hash that no longer matches its
    /// contents. That is the whole point of a tampering fixture, and it is
    /// spelled out here rather than hidden behind a convenience method —
    /// `Event::edit_and_seal` deliberately re-seals and cannot express this.
    fn tamper(event: &Event, edit: impl FnOnce(&mut EventWire)) -> Event {
        let carried = event.hash().map(str::to_string);
        let mut wire = event.clone().into_wire();
        edit(&mut wire);
        wire.hash = carried;
        Event::try_from(wire).expect("still a valid wire")
    }

    fn chain_of(n: u64) -> Vec<Event> {
        let mut chain = Vec::new();
        for seq in 1..=n {
            let mut payload = OpenPayload::new();
            payload.insert("i".into(), serde_json::json!(seq));
            let event = Event::new(
                "sess-1",
                seq,
                "2026-09-02T10:11:12.345Z",
                Source::Thruline,
                Payload::Note(payload),
                "placeholder",
            )
            .expect("constructible");
            append(&mut chain, event).expect("appends");
        }
        chain
    }

    #[test]
    fn the_streaming_walker_and_the_slice_verifier_agree() {
        // Two implementations of one rule set is a drift generator, so they are
        // held together here rather than by inspection. Both the accepting case
        // and every refusing case: a walker that accepted everything would pass
        // a test that only checked the happy path.
        let good = chain_of(6);
        let mut walker = ChainWalker::new("sess-1");
        for event in &good {
            walker.push(event).expect("accepts");
        }
        assert_eq!(
            walker.finish().expect("head"),
            verify_chain(&good).expect("head")
        );

        let mut broken = chain_of(4);
        broken.remove(1);
        let mut foreign = chain_of(3);
        foreign[2] = tamper(&foreign[2], |w| w.session_id = "other".into());
        let mut tampered = chain_of(3);
        tampered[1] = tamper(&tampered[1], |w| w.seq += 100);

        for (name, chain) in [
            ("deleted", broken),
            ("foreign session", foreign),
            ("tampered", tampered),
            ("empty", Vec::new()),
        ] {
            let slice = verify_chain(&chain);
            let mut walker = ChainWalker::new("sess-1");
            let streamed = chain
                .iter()
                .try_for_each(|e| walker.push(e))
                .and_then(|()| walker.finish().map(|_| ()));
            assert_eq!(
                slice.is_err(),
                streamed.is_err(),
                "{name}: the walker and the slice verifier disagree about validity"
            );
            if let (Err(a), Err(b)) = (&slice, &streamed) {
                assert_eq!(
                    std::mem::discriminant(a),
                    std::mem::discriminant(b),
                    "{name}: different rejection reasons — {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn a_well_formed_chain_verifies_to_its_last_hash() {
        let chain = chain_of(5);
        let head = verify_chain(&chain).expect("verifies");
        assert_eq!(head.event_count, 5);
        assert_eq!(head.last_seq, 5);
        assert_eq!(head.session_id, "sess-1");
        assert_eq!(Some(head.head_hash.as_str()), chain[4].hash());
        assert_eq!(chain[0].prev_hash(), genesis_prev_hash("sess-1"));
    }

    #[test]
    fn genesis_binds_a_chain_to_its_session() {
        // Lifting a whole chain into another session must not verify.
        let chain: Vec<Event> = chain_of(3)
            .into_iter()
            .map(|e| tamper(&e, |w| w.session_id = "sess-2".into()))
            .collect();
        let resealed: Vec<Event> = chain
            .into_iter()
            .map(|e| e.seal().expect("seals"))
            .collect();
        // Every event's own hash is now valid again, and the internal links
        // still refer to the old hashes — so the break is at index 1 — but the
        // point is that index 0's genesis no longer matches either.
        assert_ne!(resealed[0].prev_hash(), genesis_prev_hash("sess-2"));
        assert!(matches!(
            verify_chain(&resealed),
            Err(ChainError::Broken { index: 0, .. })
        ));
    }

    #[test]
    fn deleting_an_event_breaks_the_chain() {
        let chain = chain_of(5);
        let mut short = chain.clone();
        short.remove(2);
        assert!(matches!(
            verify_chain(&short),
            Err(ChainError::Broken { index: 2, .. })
        ));
    }

    #[test]
    fn reordering_two_events_breaks_the_chain() {
        // Swapping indices 1 and 2 gives seq order 1,3,2,4,5. The `seq` check
        // at index 1 passes (3 follows 1) — it is the *link* that refuses,
        // because event 3's prev_hash names event 2, which is no longer behind
        // it. Reported at index 1, the first position that cannot follow.
        let mut chain = chain_of(5);
        chain.swap(1, 2);
        assert!(
            matches!(
                verify_chain(&chain),
                Err(ChainError::Broken { index: 1, .. })
            ),
            "got {:?}",
            verify_chain(&chain)
        );
    }

    #[test]
    fn a_correctly_linked_chain_with_a_repeated_seq_is_still_refused() {
        // The control that actually reaches `OutOfOrder`. Reordering never
        // does: the link always breaks first, so a test that reordered and
        // asserted `OutOfOrder` would be asserting an arm it cannot reach —
        // and the arm could be deleted without the suite noticing.
        //
        // This is the reachable case: a producer that chains correctly but
        // re-uses a `seq`. The ledger's UNIQUE(session_id, seq) would refuse
        // it; a JSONL file on disk has no such constraint, so the verifier
        // must.
        let mut chain = chain_of(2);
        let dup = Event::new(
            "sess-1",
            2, // same seq as the event before it
            "2026-09-02T10:11:12.345Z",
            Source::Thruline,
            Payload::Note(OpenPayload::new()),
            "placeholder",
        )
        .expect("constructible");
        append(&mut chain, dup).expect("appends");
        assert!(
            matches!(
                verify_chain(&chain),
                Err(ChainError::OutOfOrder {
                    index: 2,
                    seq: 2,
                    previous: 2
                })
            ),
            "got {:?}",
            verify_chain(&chain)
        );
    }

    #[test]
    fn inserting_a_forged_event_breaks_the_chain() {
        let mut chain = chain_of(3);
        let forged = Event::new(
            "sess-1",
            2,
            "2026-09-02T10:11:12.345Z",
            Source::Thruline,
            Payload::Note(OpenPayload::new()),
            chain[0].hash().unwrap_or_default().to_string(),
        )
        .expect("constructible")
        .seal()
        .expect("seals");
        // The forgery itself is internally consistent; the event after it is
        // what refuses to follow.
        chain.insert(1, forged);
        assert!(matches!(
            verify_chain(&chain),
            Err(ChainError::OutOfOrder { .. } | ChainError::Broken { .. })
        ));
    }

    #[test]
    fn events_from_two_sessions_are_not_one_chain() {
        let mut chain = chain_of(3);
        chain[2] = tamper(&chain[2], |w| w.session_id = "sess-9".into());
        assert!(matches!(
            verify_chain(&chain),
            Err(ChainError::ForeignSession { index: 2, .. })
        ));
    }

    #[test]
    fn an_empty_chain_is_an_error_not_a_vacuous_pass() {
        assert!(matches!(verify_chain(&[]), Err(ChainError::Empty)));
    }

    #[test]
    fn an_unsealed_event_cannot_be_appended_onto() {
        let mut chain = chain_of(2);
        // `into_wire` drops the hash, so this is simply the unsealed form.
        chain[1] = Event::try_from(chain[1].clone().into_wire()).expect("valid wire");
        let next = Event::new(
            "sess-1",
            3,
            "2026-09-02T10:11:12.345Z",
            Source::Thruline,
            Payload::Note(OpenPayload::new()),
            "placeholder",
        )
        .expect("constructible");
        assert!(matches!(
            append(&mut chain, next),
            Err(ChainError::Event {
                source: EventError::Unsealed(_),
                ..
            })
        ));
    }
}
