//! G1 — the speaker path's own extraction gate, written from the first
//! precision gauntlet's adjudication (2026-08-31).
//!
//! The gauntlet replayed 29 real sessions through the speaker verbs and put
//! every emitted line in front of two independent adjudicators (plus a
//! tie-breaker): 27 of 30 lines were false accusations, 1 was ambiguous, 2
//! were true. The zero-false-accusation bar failed decisively, and the
//! failure decomposed into named mechanisms:
//!
//! * **M2 — evidence-matcher blindness.** Proof sat inside the scanned rows
//!   in shapes the matcher does not read (`TSC_EXIT=0`, `TOTAL 80 checks —
//!   80 pass`, `selftest: PASS`, `deno check`, deploy output, push-hook
//!   output, browser saves). Handled by the window-accounting guard in
//!   [`crate::speaker::EvidenceIndex`], not here.
//! * **M3 — extraction misfires.** Citation footers, relayed memory/recon
//!   content, admissions of error, standing-debt reports, user-directed
//!   imperatives, and past-tense completed work were bound as completion
//!   claims or "open promises". Killed here, shape by shape.
//! * **M4 — a taxonomy too narrow.** Claims whose honest evidence has no
//!   deterministic class (a Read, a web search, browser work, an agent's
//!   report, a deploy) were mapped onto the classes that do exist and then
//!   accused by construction. Killed here: a claim that maps to no class is
//!   an abstention with the reason on record, never an accusation.
//!
//! The governing principle (A203): an accusation requires BOTH (a) a
//! high-precision extraction — the sentence must assert, in its own words, a
//! completed outcome of a kind the deterministic tier can check — and (b) a
//! checkable evidence class whose scan is complete and definitively empty.
//! This module is (a). Everything outside its closed grammar degrades to
//! silence, and the corpus extractor (`extract.rs`, marker-v2) that annotate
//! uses is deliberately untouched: the speaker path is the only place a line
//! is put in front of a person with no human in between, so it carries the
//! stricter gate.

use crate::extract::Marker;

/// The recorded reason when a material-work claim names nothing the
/// deterministic classes can check (M4). Same abstention the A19x register
/// entries promise: outside the closed world, the machine abstains.
pub const NO_CLASS_REASON: &str = "no checkable evidence class for this claim";

/// The recorded reason for the citation/relay kill (M3).
pub const CITATION_REASON: &str = "a citation or link footer, not a completion claim";

/// The recorded reason for the debt/admission kill (M3).
pub const DEBT_REASON: &str = "reports standing debt or accepts an error, not a completion claim";

/// Which deterministic evidence classes a claim's own words engage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionClasses {
    /// The claim asserts a test/build/lint gate outcome ("all 12 tests
    /// pass", "80/80 pass, 0 fail", "clippy and fmt clean").
    pub test_run: bool,
    /// The claim asserts a commit/checkpoint/push the speaker made.
    pub commit: bool,
    /// File-like artifact tokens (path, dotted, or snake_case shapes) a
    /// first-person perfective modification claim names. Empty = the edit
    /// class is not engaged.
    pub file_artifacts: Vec<String>,
}

impl CompletionClasses {
    pub fn any(&self) -> bool {
        self.test_run || self.commit || !self.file_artifacts.is_empty()
    }
}

/// The gate's answer for one material-work claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateVerdict {
    Admitted(CompletionClasses),
    /// The claim may not be bound toward an accusation, and this is why.
    /// Refusal is silence with a receipt, never a line.
    Refused(&'static str),
}

/* ── vocabularies (closed, auditable, corpus-derived) ────────────────── */

/// Sentence openers that mark a relay of sources rather than a claim. The
/// gauntlet's citation-footer line ("Sources: [… guide](https://…)") was
/// typed as a file claim and accused; provenance is not work.
const RELAY_OPENERS: [&str; 5] = [
    "sources:",
    "source:",
    "references:",
    "citations:",
    "see also",
];

/// A sentence carrying a URL is citing, linking, or reporting — its proof,
/// when it has any, lives in fetches and searches no v0 class covers.
const URL_MARKS: [&str; 3] = ["http://", "https://", "www."];

/// Single-word negative-report tokens: any of these marks the sentence as a
/// statement of what is NOT done — standing debt, staleness, an admission —
/// and a "claimed, and no proof of completion" line against such a sentence
/// is semantically inverted (three gauntlet lines had exactly this shape).
const DEBT_TOKENS: [&str; 18] = [
    "remaining",
    "deferred",
    "stale",
    "uncommitted",
    "unmerged",
    "unresolved",
    "absent",
    "missing",
    "banned",
    "debt",
    "backlog",
    "todo",
    "todos",
    "fixme",
    "wrong",
    "concealed",
    "criticism",
    "mistake",
];

/// Multi-word negative-report and admission phrases, matched on the
/// normalised sentence.
const DEBT_PHRASES: [&str; 8] = [
    "still says",
    "still contains",
    "still shows",
    "still holds",
    "not yet",
    "left open",
    "mislead",
    "regress",
];

/// Nouns that name a machine gate. The old binder treated twenty loose words
/// (including bare "check" and "built") as test-shaped, which is how
/// "`git diff --check …` → exit 0" became an accusable test claim. This set
/// is the gates the recognizer can actually look for.
const GATE_NOUNS: [&str; 19] = [
    "test",
    "tests",
    "suite",
    "suites",
    "clippy",
    "fmt",
    "rustfmt",
    "lint",
    "lints",
    "typecheck",
    "typechecks",
    "tsc",
    "build",
    "builds",
    "compile",
    "compiles",
    "gate",
    "gates",
    "checks",
];

/// Success words. A gate noun alone engages nothing — the sentence must also
/// assert the gate came out well.
const SUCCESS_TOKENS: [&str; 8] = [
    "pass",
    "passes",
    "passed",
    "passing",
    "green",
    "clean",
    "succeeded",
    "clear",
];

const SUCCESS_PHRASES: [&str; 4] = ["0 fail", "zero fail", "no failures", "without failures"];

/// Perfective commit verbs. Engaged only agentively (leading the sentence,
/// subject elided, or with an explicit first-person subject) — "Receipt
/// committed at …" relayed mid-sentence stays out, and "the index still says
/// CP4 is UNCOMMITTED" dies at the debt kill before this runs.
const COMMIT_VERBS: [&str; 3] = ["committed", "checkpointed", "pushed"];

/// Perfective modification verbs — actions whose proof is an Edit/Write row
/// touching what the claim names. Deliberately excludes "read", "verified",
/// "checked", "deployed", "ran": those leave no evidence any v0 class holds,
/// and the gauntlet's "I've read `docs/loop/LOOP-LOG.md`" line was accused
/// precisely because the old binder demanded an edit for a read.
const MODIFICATION_VERBS: [&str; 24] = [
    "added",
    "edited",
    "wrote",
    "rewrote",
    "created",
    "updated",
    "fixed",
    "implemented",
    "removed",
    "deleted",
    "renamed",
    "moved",
    "refactored",
    "wired",
    "extended",
    "replaced",
    "hardened",
    "tightened",
    "changed",
    "patched",
    "appended",
    "adjusted",
    "corrected",
    "restored",
];

/// First-person future commitments — the only shape a promise line may wear
/// (A203 task 3). Past-tense event reports ("Pushing this checkpoint ran the
/// hook"), present-state descriptions ("Now `X` is scoped per guide"), and
/// imperatives addressed to the user ("Test it live…") were all bound as
/// open promises in the gauntlet; none of them contains one of these.
const FIRST_PERSON_FUTURE: [&str; 6] = [
    "i'll ",
    "i will ",
    "we'll ",
    "we will ",
    "i'm going to ",
    "i am going to ",
];

/* ── helpers ─────────────────────────────────────────────────────────── */

/// Lowercased, markdown emphasis stripped — phrase matching must see
/// "**still contains**" as "still contains".
fn normalise(text: &str) -> String {
    text.replace(['*', '_', '#'], "").to_ascii_lowercase()
}

/// Word tokens, apostrophes kept so "i've" survives.
fn tokens(low: &str) -> Vec<&str> {
    low.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .collect()
}

/// An explicit first-person subject anywhere in the sentence.
fn first_person(words: &[&str]) -> bool {
    words
        .iter()
        .any(|w| matches!(*w, "i" | "i've" | "we" | "we've"))
}

/// `d+/d+` anywhere — the ratio shape ("80/80", "12/12").
fn has_ratio(low: &str) -> bool {
    let chars: Vec<char> = low.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c != '/' {
            continue;
        }
        let before = i > 0 && chars[i - 1].is_ascii_digit();
        let after = chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if before && after {
            return true;
        }
    }
    false
}

fn success_asserted(low: &str, words: &[&str]) -> bool {
    words.iter().any(|w| SUCCESS_TOKENS.contains(w))
        || SUCCESS_PHRASES.iter().any(|p| low.contains(p))
}

/// A token an Edit/Write row's path could satisfy: a path, a dotted name, or
/// a snake_case identifier. (Same shape rule the binder has always matched
/// with; hoisted here so the gate and the binder agree on what "file-like"
/// means.)
pub(crate) fn file_like(token: &str) -> bool {
    token.contains('/') || token.contains('.') || token.contains('_')
}

/* ── the gate ────────────────────────────────────────────────────────── */

/// Admit or refuse one material-work claim for the accusation path.
///
/// `artifacts` is the marker-v2 extractor's lexical-gate token list for the
/// same sentence; the gate filters it down to file-like tokens for the edit
/// class rather than re-deriving it.
pub fn admit_completion(text: &str, artifacts: &[String]) -> GateVerdict {
    let low = normalise(text);
    let words = tokens(&low);

    // M3 kill 1 — citation / relay shapes.
    let trimmed = low.trim_start_matches(|c: char| !c.is_alphanumeric() && c != ':');
    if RELAY_OPENERS.iter().any(|p| trimmed.starts_with(p))
        || URL_MARKS.iter().any(|m| low.contains(m))
    {
        return GateVerdict::Refused(CITATION_REASON);
    }

    // M3 kill 2 — debt reports and admissions.
    if words.iter().any(|w| DEBT_TOKENS.contains(w)) || DEBT_PHRASES.iter().any(|p| low.contains(p))
    {
        return GateVerdict::Refused(DEBT_REASON);
    }

    let mut classes = CompletionClasses::default();

    // Test/build gate class: a ratio or a gate noun, PLUS an asserted
    // success. "scores 6/6" (a recon measurement) engages nothing; "80/80
    // pass" and "clippy clean" do.
    let success = success_asserted(&low, &words);
    if success && (has_ratio(&low) || words.iter().any(|w| GATE_NOUNS.contains(w))) {
        classes.test_run = true;
    }

    // Commit class, agentive forms only.
    let leads_commit = words.first().is_some_and(|w| COMMIT_VERBS.contains(w));
    let fp_commit = first_person(&words) && words.iter().any(|w| COMMIT_VERBS.contains(w));
    if leads_commit || fp_commit {
        classes.commit = true;
    }

    // Edit class: a perfective modification verb in an agentive position
    // (leading, subject elided — "Fixed the panel wiring in `layout.rs`" —
    // or with an explicit first-person subject), naming a file-like
    // artifact. Passive relays ("The twin is built and shelved", "The hero
    // panel is templated from … `lib/day-brief.ts`") have neither and stay
    // out.
    let leads_mod = words
        .first()
        .is_some_and(|w| MODIFICATION_VERBS.contains(w));
    let fp_mod = first_person(&words) && words.iter().any(|w| MODIFICATION_VERBS.contains(w));
    if leads_mod || fp_mod {
        classes.file_artifacts = artifacts.iter().filter(|a| file_like(a)).cloned().collect();
    }

    if classes.any() {
        GateVerdict::Admitted(classes)
    } else {
        // M4 — outside the closed world the machine abstains, with the
        // reason on record.
        GateVerdict::Refused(NO_CLASS_REASON)
    }
}

/// May this proposal become an "left open when the session ended" line?
///
/// Only a forward-looking first-person commitment qualifies. The marker is
/// checked as well as the words: gerund narration ("Pushing this checkpoint
/// ran the hook…"), "Now …" state descriptions, and imperatives are
/// structurally not commitments even when they mention the future, and the
/// gauntlet bound one of each as an open promise.
pub fn admissible_promise(text: &str, marker: Marker) -> bool {
    if !matches!(marker, Marker::Intent) {
        return false;
    }
    let low = normalise(text);
    FIRST_PERSON_FUTURE.iter().any(|p| low.contains(p))
}
