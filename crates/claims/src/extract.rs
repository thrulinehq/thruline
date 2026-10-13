//! BUILD-SPEC §5.1–§5.2 — the rules-first extractor, and §3.5 promises.

use crate::claim::{
    derive_id, Claim, ClaimKind, Extractor, Promise, PromiseKind, PromiseStatus, Span,
};
use crate::rules::{is_negated, normalize, scope_of, sentences, Table};

/// One text an extractor reads: the message, and the event it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub event_id: String,
    pub session_id: String,
    pub text: String,
}

/// What one pass over a session's texts found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extraction {
    pub claims: Vec<Claim>,
    pub promises: Vec<Promise>,
    /// Sentences a pattern matched but negation refused. **Kept, and counted.**
    ///
    /// Not evidence of anything and never read by a verifier — it exists
    /// because "the extractor found nothing" and "the extractor found something
    /// and declined it" are different facts, and a suppression nobody can see
    /// is how an extractor silently stops working.
    pub negated: Vec<(String, ClaimKind)>,
}

/// Extract claims and promises from a session's texts, in order.
pub fn extract(sources: &[Source], table: &Table) -> Extraction {
    let mut out = Extraction::default();
    for source in sources {
        for (offset, sentence) in sentences(&source.text) {
            let (normalised, map) = normalize(sentence);
            extract_claims(source, offset, sentence, &normalised, &map, table, &mut out);
            extract_promise(source, offset, sentence, &normalised, &mut out);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn extract_claims(
    source: &Source,
    offset: usize,
    sentence: &str,
    normalised: &str,
    map: &[usize],
    table: &Table,
    out: &mut Extraction,
) {
    // Ordered: the first pattern that matches wins, so a sentence yields at
    // most one claim per kind and the table's order is meaningful.
    let mut seen_kinds: Vec<ClaimKind> = Vec::new();
    for pattern in table.patterns() {
        if seen_kinds.contains(&pattern.kind) {
            continue;
        }
        let Some(at) = normalised.find(&pattern.phrase) else {
            continue;
        };
        if is_negated(normalised, at) {
            out.negated
                .push((sentence.trim().to_string(), pattern.kind));
            seen_kinds.push(pattern.kind);
            continue;
        }
        let start = offset + map.get(at).copied().unwrap_or(0);
        let end = offset
            + map
                .get(at + pattern.phrase.len())
                .copied()
                .unwrap_or(sentence.len());
        let text = sentence.trim().to_string();
        out.claims.push(Claim {
            id: derive_id(
                "clm_",
                &format!(
                    "{}|{}|{}|{}",
                    source.session_id,
                    source.event_id,
                    start,
                    pattern.kind.as_str()
                ),
            ),
            session_id: source.session_id.clone(),
            kind: pattern.kind,
            text,
            span: Span {
                event_id: source.event_id.clone(),
                offset: (start, end),
            },
            args: scope_of(normalised, pattern.scope),
            extractor: Extractor::Rules,
        });
        seen_kinds.push(pattern.kind);
    }
}

/// §5.2's promise list.
///
/// A promise is forward-looking, so the cues are about *tense and intent*
/// rather than about outcome. Kept separate from the claim table because the
/// two answer different questions and sharing a table would let an edit to one
/// silently change the other.
const PROMISE_CUES: &[(&str, PromiseKind)] = &[
    ("todo", PromiseKind::Todo),
    ("follow-up", PromiseKind::FollowUp),
    ("follow up", PromiseKind::FollowUp),
    ("in a future pr", PromiseKind::FollowUp),
    ("in a separate pr", PromiseKind::FollowUp),
    ("next step", PromiseKind::FollowUp),
    ("i'll ", PromiseKind::Deferred),
    ("i will ", PromiseKind::Deferred),
    ("we'll ", PromiseKind::Deferred),
    ("should be added", PromiseKind::Deferred),
    ("left as an exercise", PromiseKind::Deferred),
    ("not yet", PromiseKind::Deferred),
    ("for now", PromiseKind::Deferred),
    ("later", PromiseKind::Deferred),
    ("remaining", PromiseKind::Deferred),
    ("i assumed", PromiseKind::Assumption),
    ("assuming ", PromiseKind::Assumption),
];

fn extract_promise(
    source: &Source,
    offset: usize,
    sentence: &str,
    normalised: &str,
    out: &mut Extraction,
) {
    for (cue, kind) in PROMISE_CUES {
        if !normalised.contains(cue) {
            continue;
        }
        let text = sentence.trim().to_string();
        out.promises.push(Promise {
            id: derive_id(
                "prm_",
                &format!("{}|{}|{}", source.session_id, source.event_id, offset),
            ),
            session_id: source.session_id.clone(),
            text,
            kind: *kind,
            // Opened. Nothing in M1 closes a promise automatically; a closer
            // needs the follow-up session, which is M4's cross-session view.
            status: PromiseStatus::Open,
            opened_event_id: source.event_id.clone(),
            closed_event_id: None,
            closed_reason: None,
        });
        // One promise per sentence: the cues overlap heavily ("I'll follow up
        // later" hits three) and three rows for one sentence is noise.
        return;
    }
}

/// **F5 — zero claims and proven-good were the same green.**
///
/// R, verbatim: *"Report completion in ordinary English that the substring
/// table does not contain ("the change is in place", "that should do it",
/// "diff applied"). Claim count = 0, and every gate has nothing to iterate over
/// while the failing run sits in the receipt unread. Reviewer sees … identical
/// to a proven-good session: `success`, green badge."*
///
/// Measured on `b5335c0`: that message over a real edit produced 0 claims, 0
/// verdicts, `conclusion: success`, `verified: true`.
///
/// # Why this is not a bigger phrase table
///
/// R's explicit instruction was **not** to fix it by enlarging the table.
/// Enlarging it is an arms race against English and it loses: every phrase
/// added is one paraphrase away from being evaded, and the failure mode of a
/// missed paraphrase is silent green. So the fix does not try to recognise the
/// sentence at all. It recognises the *situation*: **the session changed files
/// and stopped without a claim this build can name.**
///
/// Stopping is itself a completion assertion — the agent handed the work back.
/// It is recorded as an unqualified [`ClaimKind::TaskComplete`], which has no
/// verifier in this build and is therefore `unverifiable`: the honest word.
/// The absence of a claim must not read as the absence of a problem.
///
/// # It can only move a verdict DOWN
///
/// It fires only when `extraction.claims` is empty, so it can never displace a
/// claim that was found, and the verdict it earns is `unverifiable`, which
/// [`crate`]'s consumers may only combine downward. A session that changed
/// nothing has nothing unproved and is left exactly as it was — this is a
/// floor, not a blanket pessimism.
///
/// `anchor_event_id` is the event whose message was read; the claim's span
/// points into it, so the receipt's claim still traces to a row in the chain.
pub fn unclaimed_completion(
    session_id: &str,
    anchor_event_id: &str,
    final_message: &str,
    extraction: &Extraction,
    files_changed: usize,
) -> Option<Claim> {
    if !extraction.claims.is_empty() || files_changed == 0 {
        return None;
    }
    // The last non-empty sentence: the smallest honest excerpt of what the
    // agent actually handed back. §0.8 — the receipt carries as little prose as
    // it can while still letting a human see the sentence.
    let (offset, text) = sentences(final_message)
        .into_iter()
        .next_back()
        .map(|(o, s)| (o, s.trim().to_string()))
        .unwrap_or((0, String::new()));
    Some(Claim {
        id: derive_id(
            "clm_",
            // `unclaimed` where a matched claim carries a byte offset, so the
            // two derivations cannot collide.
            &format!(
                "{session_id}|{anchor_event_id}|unclaimed|{}",
                ClaimKind::TaskComplete.as_str()
            ),
        ),
        session_id: session_id.to_string(),
        kind: ClaimKind::TaskComplete,
        text,
        span: Span {
            event_id: anchor_event_id.to_string(),
            offset: (offset, final_message.len()),
        },
        args: crate::claim::ClaimArgs::default(),
        extractor: Extractor::Rules,
    })
}
