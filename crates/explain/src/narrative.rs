//! §6.9 — honest-failure recognition, **as narrative only** (ruling C.9).
//!
//! Nothing here reaches a verdict, a gate, or a receipt. It exists so §10 can
//! say "the agent reported failure honestly" beside the claims table rather
//! than inside it.
//!
//! # Why beside and not inside
//!
//! §6.9 as written let these markers suppress a gate failure. That is prose
//! changing an outcome, and it is trivially exploitable: an agent that claims
//! success and appends "I couldn't get the tests running" would have its
//! missing evidence excused by a sentence it chose to write.
//!
//! The one real case behind §6.9 — a sentence that IS an honest failure but
//! from which a naive matcher extracts a claim — is a negation bug, and it is
//! fixed in `claims::rules`, where it changes *whether a claim exists*.

use serde::{Deserialize, Serialize};

/// What the final message said about failing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HonestFailure {
    /// The agent said it could not do something.
    pub reported_failure: bool,
    /// …and made no completion claim alongside it.
    pub without_completion_claim: bool,
    /// The sentences that say so, for display.
    pub sentences: Vec<String>,
}

impl HonestFailure {
    /// The sentence §10 prints. `None` when there is nothing to say.
    pub fn narrative(&self) -> Option<String> {
        if !self.reported_failure {
            return None;
        }
        Some(if self.without_completion_claim {
            "the agent reported failure honestly and claimed nothing it did not do".to_string()
        } else {
            // Both at once is worth naming: it is the shape that made §6.9
            // exploitable, and a reader should see it rather than have it
            // quietly excuse the claim.
            "the agent reported a failure AND made a completion claim in the same \
             message — the claim is judged on evidence regardless"
                .to_string()
        })
    }
}

const MARKERS: &[&str] = &[
    "couldn't",
    "could not",
    "was unable",
    "unable to",
    "failed to",
    "i failed",
    "not able to",
    "did not manage",
    "didn't manage",
    "gave up",
    "blocked on",
    "ran out of",
];

/// Recognise honest-failure markers in a final message.
pub fn honest_failure(message: &str, has_completion_claim: bool) -> HonestFailure {
    let lower = message.to_ascii_lowercase();
    let sentences: Vec<String> = message
        .split_inclusive(['.', '!', '?', '\n'])
        .filter(|s| {
            let l = s.to_ascii_lowercase();
            MARKERS.iter().any(|m| l.contains(m))
        })
        .map(|s| s.trim().to_string())
        .collect();
    let reported = MARKERS.iter().any(|m| lower.contains(m));
    HonestFailure {
        reported_failure: reported,
        without_completion_claim: reported && !has_completion_claim,
        sentences,
    }
}
