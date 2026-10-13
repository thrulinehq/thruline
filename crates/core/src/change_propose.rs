//! Proposed criteria, a suggested check profile, and the one act that accepts
//! them (brief `B5.1`–`B5.10`, `B11.5`, `B3.16`; `docs/loop/03-USER-LENS.md`
//! rule 8).
//!
//! # What this module refuses to do
//!
//! It does not write a criterion. Every word of every proposal is a span of
//! text a person already wrote — the Change's own purpose, the original prompt,
//! or the opening request of a session someone confirmed belongs to this
//! Change. The only character this module adds to a person's words is the full
//! stop that ends the sentence. A sentence that cannot be cut into its parts
//! is **kept as a note, not a criterion**, with the part that was missing
//! named. Nothing is rephrased,
//! summarised, or inferred, and no model is reachable from here.
//!
//! That is the whole answer to `B5.5`: a drafter that cannot invent words
//! cannot silently weaken a requirement, and a drafter that cannot write to
//! `change_scope_revisions` cannot silently accept one. [`accept_with`] (and
//! [`accept`], which is it with nothing added) is the only function here that
//! writes anything, it requires an actor, and it appends.
//!
//! # The three parts
//!
//! `subject · condition · observable outcome`. A criterion must say what it is
//! about and what someone would see, and keeps the condition it states. A
//! sentence that states no condition is proposed with an empty one — never an
//! invented one — unless it describes how things stand now (`QUAL-F11`: "The
//! total shows the sum of the prices." is what was asked, not a wish). "make
//! retries reliable"
//! has a subject and nothing else, so it is never a criterion — and
//! [`observable_shape`] says which part was missing rather than failing
//! silently. This is the mechanical form of `B5.1` and of the brief's own
//! example at `B5.2`.
//!
//! # What is derived and what is stored
//!
//! Proposals are **derived at read**, every time, from stored rows. There is no
//! proposals table and this module adds no DDL: a proposal is a reading of the
//! record, and a reading that disagrees with the record is a bug that a stored
//! copy would hide. What acceptance stores is the thing a person agreed to, in
//! the two tables T-2-1 already built — a new scope version, and its acceptance
//! record with actor, time, version and rationale (`B5.6`).
//!
//! # Accepting is not approving
//!
//! `B5.7`, held by the shape of the store rather than by a check: this module
//! can reach `change_scope_revisions` and `change_scope_acceptances` and no
//! other table. There is no decision or approval row for it to touch.
//!
//! No `unwrap`, no `expect`, no network, no `unsafe`.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::chain::sha256_hex;
use crate::change::{
    self, mask_list, mask_text, AcceptedScope, AssociationProvenance, ScopeAcceptance,
};
use crate::change_link;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The served shape's version for everything in this module.
pub const PROPOSAL_VIEW_SCHEMA_VERSION: u32 = 1;

/// How many versions of "what counts as done" one Change may hold.
///
/// Bounded because `docs/loop/03-USER-LENS.md` rule 13 is about what this
/// product quietly keeps on a person's disk: an unbounded version series on an
/// append-only table grows forever and nothing would ever say so. The bound is
/// stated on every proposals view, and the refusal at the limit names the number
/// and what to do instead — a disabled control that explains itself.
pub const MAX_SCOPE_VERSIONS: u32 = 20;

/// The longest a single criterion may be. A paragraph is not one thing to
/// check, and a person asked to agree to a paragraph agrees to none of it.
pub const MAX_CRITERION_CHARS: usize = 240;

/// The shortest a sentence may be and still be offered as one thing to
/// check. A shorter sentence ("Retry now.", "Do it.") is still the person's
/// own writing: it is kept as a note that says it is too short to check on
/// its own, never dropped (`QUAL-F11-LAUNCH-R5`).
const MIN_SENTENCE_CHARS: usize = 12;

// ---------------------------------------------------------------------------
// The observable shape
// ---------------------------------------------------------------------------

/// Words that mean "here is something someone could watch happen".
///
/// A closed table, pinned by `crates/core/tests/change_propose.rs`, for the same
/// reason [`crate::change_link::EDIT_TOOLS`] is: the set is a claim about which
/// sentences this product is willing to call checkable, and a silent addition to
/// it would silently widen that claim. Some of these are also nouns in English
/// ("logs", "records"); that ambiguity cannot be removed by a bigger list, and
/// it is safe in one direction only — a noun match produces a sentence that then
/// fails the condition or subject test and becomes a note, never a criterion
/// nobody could check.
///
/// Entries may be phrases. Matching is by word sequence, earliest position
/// first, longest at that position.
pub const OUTCOME_MARKERS: &[&str] = &[
    "aborts",
    "appears",
    "are recorded",
    "are refused",
    "are rejected",
    // This and "is shown" are the passive of "shows", which this table already
    // holds: "a delivery … is shown as failed" names something someone could
    // watch (`QUAL-F11`). A deliberate widening, not a silent one.
    "are shown",
    "arrives",
    "contains",
    "creates",
    "deletes",
    "enqueue",
    "enqueues",
    "equals",
    "exits",
    "fails",
    "increments",
    "is logged",
    "is recorded",
    "is refused",
    "is rejected",
    "is returned",
    "is shown",
    "is written",
    "keeps",
    "logs",
    "matches",
    "passes",
    "raises",
    "receives",
    "records",
    "refuses",
    "rejects",
    "remains",
    "renders",
    "reports",
    "responds",
    "retains",
    "returns",
    "runs",
    "sends",
    "shows",
    "starts",
    "stays",
    "stops",
    "stores",
    "succeeds",
    "times out",
    "writes",
];

/// Ways a person says "not", which belong to the outcome rather than in front
/// of it: "do not enqueue duplicate work" is one outcome, and dropping the
/// negation would reverse the requirement.
pub const NEGATION_MARKERS: &[&str] = &[
    "are not", "aren't", "can't", "cannot", "do not", "does not", "doesn't", "don't", "is not",
    "isn't", "must not", "never", "no", "will not", "won't",
];

/// Ways a person says "when". A criterion with no condition holds always, which
/// is almost never what was meant and is never checkable as written.
pub const CONDITION_MARKERS: &[&str] = &[
    "after",
    "as soon as",
    "before",
    "during",
    "each time",
    "every time",
    "for",
    "for each",
    "given",
    "if",
    "on",
    "once",
    "unless",
    "when",
    "whenever",
    "while",
    "with",
];

/// Ways a person says "as things stand now". A sentence with no condition that
/// carries one of these describes the present — "the handler currently returns
/// an error" — and proposing it as something that counts as done would turn a
/// description of the defect into a requirement nobody asked for. It stays a
/// note, as every sentence with no condition did before `QUAL-F11`.
pub const PRESENT_STATE_MARKERS: &[&str] = &[
    "at present",
    "at the moment",
    "currently",
    "presently",
    "right now",
];

/// Words that carry no subject on their own.
const SUBJECT_STOPWORDS: &[&str] = &[
    "a", "all", "an", "and", "any", "each", "every", "it", "its", "one", "some", "that", "the",
    "their", "they", "this", "those", "three", "two",
];

/// One sentence cut into the three parts, in the order a person reads them.
/// An empty `condition` means the person stated none; one is never invented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservableShape {
    pub subject: String,
    pub condition: String,
    pub outcome: String,
}

impl ObservableShape {
    /// Does the outcome carry a negation? A criterion that says "does not
    /// enqueue duplicate work" and a criterion that says "enqueues work" are
    /// opposites, and [`diff_against_accepted`] has to be able to tell.
    pub fn outcome_is_negated(&self) -> bool {
        let words = words_of(&self.outcome);
        let lower: Vec<&str> = words.iter().map(|w| w.lower.as_str()).collect();
        (0..lower.len()).any(|i| phrase_at(&lower, i, NEGATION_MARKERS).is_some())
    }

    /// The pairing key: the subject with the words that carry no meaning of
    /// their own removed. Two sentences about the same thing pair here even when
    /// the rest of them differs, which is what makes a weakening visible.
    pub fn subject_key(&self) -> String {
        words_of(&self.subject)
            .iter()
            .map(|w| {
                w.lower
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
            })
            .filter(|w| !w.is_empty() && !SUBJECT_STOPWORDS.contains(&w.as_str()))
            .collect::<Vec<_>>()
            .join("-")
    }
}

/// Why a sentence is not a criterion. Each maps to one copy-file line, so a
/// person is never told "no" without being told what was missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeRefusal {
    /// Nothing in it says what someone would see.
    NoOutcome,
    /// Nothing in it says when it should hold.
    NoCondition,
    /// Nothing in it says what the sentence is about.
    NoSubject,
    /// It is a paragraph, not one thing to check.
    TooLong,
    /// The masker removed part of it, so it is no longer the sentence anybody
    /// wrote.
    Masked,
    /// It is a piece of a sentence, not a sentence: it starts in the middle of
    /// a line in lower case, it trails off without a stop, it opens a
    /// quotation it never closes (`QUAL-F11-LAUNCH-R2`), or it holds a quote
    /// mark that closes nothing (`QUAL-F11-LAUNCH-R3`).
    Fragment,
    /// It is shorter than `MIN_SENTENCE_CHARS`: too short to check on its
    /// own (`QUAL-F11-LAUNCH-R5`).
    TooShort,
    /// It repeats, word for word, an earlier line of the same request, which
    /// is where it is proposed or noted already (`QUAL-F11-LAUNCH-R5`).
    Repeated,
    /// It repeats, word for word, a line of another request this Change is
    /// read out of — its purpose, the prompt it points at, or a confirmed
    /// session's opening — which is where it is proposed or noted already
    /// (`QUAL-F11-LAUNCH-R6`).
    RepeatsAnotherRequest,
}

impl ShapeRefusal {
    pub fn slug(self) -> &'static str {
        match self {
            Self::NoOutcome => "note_no_outcome",
            Self::NoCondition => "note_no_condition",
            Self::NoSubject => "note_no_subject",
            Self::TooLong => "note_too_long",
            Self::Masked => "note_masked",
            Self::Fragment => "note_fragment",
            Self::TooShort => "note_too_short",
            Self::Repeated => "note_repeats_an_earlier_line",
            Self::RepeatsAnotherRequest => "note_repeats_a_line_of_another_request",
        }
    }
}

struct Word {
    lower: String,
    start: usize,
}

fn words_of(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(from) = start.take() {
                out.push(Word {
                    lower: text[from..index].to_ascii_lowercase(),
                    start: from,
                });
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(from) = start {
        out.push(Word {
            lower: text[from..].to_ascii_lowercase(),
            start: from,
        });
    }
    out
}

/// A word with its surrounding punctuation stripped, for table comparison.
fn bare(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
}

/// The longest entry of `table` that begins at word `index`, as a word count.
fn phrase_at(words: &[&str], index: usize, table: &[&str]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for entry in table {
        let parts: Vec<&str> = entry.split_whitespace().collect();
        if parts.is_empty() || index + parts.len() > words.len() {
            continue;
        }
        let hit = parts
            .iter()
            .enumerate()
            .all(|(offset, part)| bare(words[index + offset]) == *part);
        if hit && best.is_none_or(|b| parts.len() > b) {
            best = Some(parts.len());
        }
    }
    best
}

/// The criterion text for one sentence: the person's own sentence, trimmed,
/// with exactly one full stop at the end.
///
/// **The text is never reordered.** An earlier draft rendered the three parts in
/// canonical order and produced "The upload panel when the network drops. shows
/// an error message" — a sentence nobody wrote, from a tool whose whole promise
/// is that it does not write sentences. The parts exist so a reader can see
/// *why* a sentence is checkable; the sentence a person is asked to accept is
/// their own.
pub fn criterion_text(sentence: &str) -> String {
    let core = core_of(sentence);
    format!("{core}.")
}

/// The sentence with its surrounding whitespace and its trailing sentence
/// punctuation removed, so no part ever carries a stray full stop.
fn core_of(sentence: &str) -> &str {
    sentence
        .trim()
        .trim_end_matches(['.', '!', '?', ';', ',', ' '])
}

/// Find the outcome: the earliest word that names something someone could
/// watch, extended backwards over a negation so "do not enqueue" stays one
/// outcome. Returns the byte offset the outcome starts at.
fn outcome_start_in(text: &str) -> Option<usize> {
    let words = words_of(text);
    let lower: Vec<&str> = words.iter().map(|w| w.lower.as_str()).collect();
    let mut index = (0..lower.len()).find(|i| phrase_at(&lower, *i, OUTCOME_MARKERS).is_some())?;
    for back in 1..=2usize {
        if index < back {
            break;
        }
        if phrase_at(&lower, index - back, NEGATION_MARKERS) == Some(back) {
            index -= back;
            break;
        }
    }
    Some(words[index].start)
}

/// The byte offset of the earliest condition marker at or after word `from`.
fn condition_start_in(text: &str, from: usize) -> Option<usize> {
    let words = words_of(text);
    let lower: Vec<&str> = words.iter().map(|w| w.lower.as_str()).collect();
    let index = (from..lower.len()).find(|i| phrase_at(&lower, *i, CONDITION_MARKERS).is_some())?;
    Some(words[index].start)
}

/// True when a subject says something: at least one word that is not a
/// stopword.
fn subject_carries_meaning(subject: &str) -> bool {
    words_of(subject).iter().any(|w| {
        let stripped: String = w.lower.chars().filter(|c| c.is_alphanumeric()).collect();
        !stripped.is_empty() && !SUBJECT_STOPWORDS.contains(&stripped.as_str())
    })
}

/// The opening of the marker [`crate::redact`] leaves behind.
///
/// Not a shape rule but a provenance one: a sentence the masker cut into is no
/// longer the person's sentence, and this module's whole promise is that it
/// proposes nobody's words but the person's. `a_sentence_the_masker_cut_into_is
/// _never_proposed` in `crates/core/tests/change_propose.rs` builds the marker
/// by running the real masker rather than by writing this string twice, so the
/// two cannot drift apart silently.
const MASK_MARKER: &str = "[masked: ";

/// Cut one sentence into `subject · condition · observable outcome`, or say
/// which part is missing.
///
/// Deliberately a *cut*, never a rewrite: every part is a contiguous span of
/// the input, so this function cannot put words in anybody's mouth.
///
/// Three orders are recognised, because all three are how people write
/// requirements, and refusing two of them would send a person to write
/// criteria by hand — the wall rule 8 forbids:
///
/// * **condition first**, with a comma: "When a delivery fails three times, the
///   record shows a single failure row";
/// * **condition in the middle**: "Two retry requests for the same delivery do
///   not enqueue duplicate work";
/// * **condition last**: "The upload panel shows an error message when the
///   network drops".
///
/// A sentence that begins with a condition word and has no comma is ambiguous
/// about where the condition ends, and ambiguity is not resolved here — it falls
/// through to the other two orders and is otherwise kept as a note.
///
/// **No condition at all** (`QUAL-F11`): "The total shows the sum of the
/// prices." is proposed with an empty condition, because that is what the
/// person asked for and inventing a "when" would put words in their mouth. It
/// needs a subject, and a result after the outcome word ("The total shows" has
/// none). A sentence that describes the present ([`PRESENT_STATE_MARKERS`]) is
/// still a note.
pub fn observable_shape(sentence: &str) -> std::result::Result<ObservableShape, ShapeRefusal> {
    if sentence.trim().chars().count() > MAX_CRITERION_CHARS {
        return Err(ShapeRefusal::TooLong);
    }
    // Refused before the cut, deliberately.
    //
    // Masking runs ahead of the cut (the `A209-C3` ordering), and a mask can eat
    // a sentence terminator along with the secret: a key written as
    // `...the key is AKIA….` loses the full stop with it and the next sentence
    // arrives welded on. The first draft proposed the welded result — a
    // "criterion" whose subject was half of somebody's complaint and a mask
    // marker, from a tool whose one promise is that it writes no sentences. It
    // is kept as a note instead, masked, with the reason said.
    if sentence.contains(MASK_MARKER) {
        return Err(ShapeRefusal::Masked);
    }
    let core = core_of(sentence);
    let words = words_of(core);
    let lower: Vec<&str> = words.iter().map(|w| w.lower.as_str()).collect();

    // Condition first: everything up to the comma is the condition.
    if phrase_at(&lower, 0, CONDITION_MARKERS).is_some() {
        if let Some(comma) = core.find(',') {
            let condition = core[..comma].trim().to_string();
            let rest = core[comma + 1..].trim();
            let Some(outcome_at) = outcome_start_in(rest) else {
                return Err(ShapeRefusal::NoOutcome);
            };
            let subject = rest[..outcome_at].trim().to_string();
            if !subject_carries_meaning(&subject) {
                return Err(ShapeRefusal::NoSubject);
            }
            return Ok(ObservableShape {
                subject,
                condition,
                outcome: rest[outcome_at..].trim().to_string(),
            });
        }
    }

    let Some(outcome_at) = outcome_start_in(core) else {
        return Err(ShapeRefusal::NoOutcome);
    };
    let prefix = core[..outcome_at].trim_end();
    let outcome_span = core[outcome_at..].trim();

    // Condition in the middle: inside the prefix, but not at its very start —
    // a prefix that is only a condition leaves no subject.
    if let Some(condition_at) = condition_start_in(prefix, 1) {
        let subject = prefix[..condition_at].trim().to_string();
        if !subject_carries_meaning(&subject) {
            return Err(ShapeRefusal::NoSubject);
        }
        return Ok(ObservableShape {
            subject,
            condition: prefix[condition_at..].trim().to_string(),
            outcome: outcome_span.to_string(),
        });
    }

    // Condition last: inside the outcome span, after the outcome word itself.
    if let Some(condition_at) = condition_start_in(outcome_span, 1) {
        let subject = prefix.trim().to_string();
        if !subject_carries_meaning(&subject) {
            return Err(ShapeRefusal::NoSubject);
        }
        return Ok(ObservableShape {
            subject,
            condition: outcome_span[condition_at..].trim().to_string(),
            outcome: outcome_span[..condition_at].trim().to_string(),
        });
    }

    // No condition stated. A leading condition word whose boundary was not
    // found is not "no condition", and a description of the present is not a
    // requirement: both stay notes.
    if phrase_at(&lower, 0, CONDITION_MARKERS).is_some()
        || (0..lower.len()).any(|i| phrase_at(&lower, i, PRESENT_STATE_MARKERS).is_some())
    {
        return Err(ShapeRefusal::NoCondition);
    }
    if !subject_carries_meaning(prefix) {
        return Err(ShapeRefusal::NoSubject);
    }
    // The outcome word must be followed by what someone would see.
    let outcome_words = words_of(outcome_span);
    let outcome_lower: Vec<&str> = outcome_words.iter().map(|w| w.lower.as_str()).collect();
    let result = (0..outcome_lower.len())
        .find_map(|i| phrase_at(&outcome_lower, i, OUTCOME_MARKERS).map(|len| i + len))
        .and_then(|after| outcome_words.get(after))
        .map(|word| &outcome_span[word.start..]);
    if !result.is_some_and(subject_carries_meaning) {
        return Err(ShapeRefusal::NoOutcome);
    }
    Ok(ObservableShape {
        subject: prefix.trim().to_string(),
        condition: String::new(),
        outcome: outcome_span.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Where a proposal came from
// ---------------------------------------------------------------------------

/// Which piece of the record a sentence was read out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AskOrigin {
    /// The Change's own purpose, as the person who opened it wrote it.
    ChangePurpose,
    /// The original prompt the Change points at.
    OriginalPrompt,
    /// The first thing a person wrote in a session someone confirmed belongs
    /// to this Change.
    SessionOpening,
}

impl AskOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChangePurpose => "change_purpose",
            Self::OriginalPrompt => "original_prompt",
            Self::SessionOpening => "session_opening",
        }
    }

    fn order(self) -> u8 {
        match self {
            Self::ChangePurpose => 0,
            Self::OriginalPrompt => 1,
            Self::SessionOpening => 2,
        }
    }
}

/// The receipt path for one proposal: which request it was read out of, and
/// exactly where in it.
///
/// `offset` and `length` are byte offsets into the **masked** text this view
/// serves, not into the stored row. Masking happens before the cut (the
/// `A209-C3` ordering), so a span can never point at something the masker
/// removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AskSource {
    /// A deterministic id for the request this came from, so the same store
    /// always names the same source.
    pub ask_id: String,
    pub origin: AskOrigin,
    pub session_id: Option<String>,
    pub event_id: Option<String>,
    pub offset: usize,
    pub length: usize,
}

fn ask_id(origin: AskOrigin, key: &str) -> String {
    let digest = sha256_hex(format!("{}\u{1f}{key}", origin.as_str()).as_bytes());
    format!("req_{}", &digest[..24])
}

/// One sentence of one request, with where it came from and whether it reads
/// as a whole sentence rather than a piece of one.
struct Candidate {
    source: AskSource,
    text: String,
    whole: bool,
    too_short: bool,
}

// ---------------------------------------------------------------------------
// The proposals
// ---------------------------------------------------------------------------

/// One proposed criterion: a sentence someone could check, the three parts it
/// was cut into, where it came from, and why it is here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposedCriterion {
    /// 1-based, stable for one reading of one store. This is what `--criterion`
    /// names.
    pub number: u32,
    pub text: String,
    pub shape: ObservableShape,
    pub subject_key: String,
    pub source: AskSource,
    /// The copy-file slug for the one-sentence reason this was proposed.
    pub reasoning_slug: &'static str,
}

/// One sentence that stayed a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeptNote {
    pub text: String,
    pub source: AskSource,
    pub refusal: ShapeRefusal,
    /// The copy-file slug for what was missing.
    pub reason_slug: &'static str,
}

// ---------------------------------------------------------------------------
// The check profile
// ---------------------------------------------------------------------------

/// The kind of work a Change is, read off the paths it covers.
///
/// The five `B5.8` names them, and no more: a sixth kind would be a claim this
/// product cannot check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Documentation,
    InterfaceBehaviour,
    BackendBehaviour,
    AuthorizationSensitive,
    DataSchema,
}

impl ChangeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Documentation => "documentation",
            Self::InterfaceBehaviour => "interface_behaviour",
            Self::BackendBehaviour => "backend_behaviour",
            Self::AuthorizationSensitive => "authorization_sensitive",
            Self::DataSchema => "data_schema",
        }
    }

    /// The copy-file slug for what this kind of work is, in Sam's words.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Documentation => "type_documentation",
            Self::InterfaceBehaviour => "type_interface_behaviour",
            Self::BackendBehaviour => "type_backend_behaviour",
            Self::AuthorizationSensitive => "type_authorization_sensitive",
            Self::DataSchema => "type_data_schema",
        }
    }
}

/// One path's kind, from the path alone.
///
/// Order matters and is stated: a migration under `auth/` is both
/// authorization-sensitive and a schema change, and both are returned. Nothing
/// here reads a file's contents — the paths are all the record holds.
pub fn types_of_path(path: &str) -> Vec<ChangeType> {
    let lower = path.to_ascii_lowercase();
    let mut out = Vec::new();
    let contains_any = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if contains_any(&["migration", "migrations", "schema", "ddl"]) || lower.ends_with(".sql") {
        out.push(ChangeType::DataSchema);
    }
    if contains_any(&[
        "auth",
        "login",
        "permission",
        "credential",
        "secret",
        "token",
        "policy",
        "acl",
    ]) {
        out.push(ChangeType::AuthorizationSensitive);
    }
    if contains_any(&["ui/", "/api/", "api/", "route", "component", "handler"])
        || [".jsx", ".tsx", ".vue", ".css", ".html"]
            .iter()
            .any(|e| lower.ends_with(e))
    {
        out.push(ChangeType::InterfaceBehaviour);
    }
    if contains_any(&["docs/", "doc/", "readme"])
        || [".md", ".rst", ".txt", ".adoc"]
            .iter()
            .any(|e| lower.ends_with(e))
    {
        out.push(ChangeType::Documentation);
    }
    if out.is_empty() {
        out.push(ChangeType::BackendBehaviour);
    }
    out
}

/// One check a profile suggests: which tool, whether it is needed before
/// approval, and whether it can run here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileCheck {
    /// The registered adapter id (`spec/integrations/support.json`), so a
    /// suggestion names something that exists rather than a category.
    pub id: &'static str,
    /// The program that would have to be on this machine.
    pub tool: &'static str,
    pub category: &'static str,
    pub mandatory: bool,
    /// Which kinds of work asked for this check.
    pub for_types: Vec<ChangeType>,
    pub availability: Availability,
    /// The copy-file slug for whether it is needed.
    pub need_slug: &'static str,
    /// The copy-file slug for whether it can run here.
    pub availability_slug: &'static str,
    /// The tool or the settings file the availability sentence names. Never a
    /// sentence built here.
    pub availability_name: String,
}

/// Whether a check could actually run on this machine.
///
/// Three answers, not two, because the baseline measured both halves: 19 of the
/// 22 registered tools are not installed here, and **none** of the three that
/// are has the reviewed settings file it needs. A profile that called those
/// three "ready" would be the green badge Sam has already stopped believing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Availability {
    Ready,
    ToolAbsent { tool: String },
    RecipeMissing { recipe: String },
}

impl Availability {
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Ready => "check_ready",
            Self::ToolAbsent { .. } => "check_not_available_here",
            Self::RecipeMissing { .. } => "check_needs_a_recipe",
        }
    }

    fn name(&self) -> String {
        match self {
            Self::Ready => String::new(),
            Self::ToolAbsent { tool } => tool.clone(),
            Self::RecipeMissing { recipe } => recipe.clone(),
        }
    }
}

/// `(kind, adapter id, program, category, mandatory)` — the whole mapping from
/// change type to checks, in one table.
///
/// Every adapter id here is one this product already has a decoder for
/// (`crates/verify/src/integrations/decode.rs`) or a registered runner
/// (`crates/verify/src/parse.rs`). A profile may not name a tool ThruLine could
/// not read the output of, because a check whose result cannot be read is not a
/// check; `B11.3` says the same thing about commands.
const PROFILE_TABLE: &[(ChangeType, &str, &str, &str, bool)] = &[
    (
        ChangeType::Documentation,
        "megalinter",
        "megalinter",
        "lint",
        false,
    ),
    (
        ChangeType::InterfaceBehaviour,
        "playwright",
        "playwright",
        "behaviour",
        true,
    ),
    (
        ChangeType::InterfaceBehaviour,
        "knip",
        "knip",
        "lint",
        false,
    ),
    (
        ChangeType::BackendBehaviour,
        "cargo-test",
        "cargo",
        "tests",
        true,
    ),
    (
        ChangeType::BackendBehaviour,
        "clippy",
        "cargo-clippy",
        "lint",
        false,
    ),
    (
        ChangeType::AuthorizationSensitive,
        "semgrep",
        "semgrep",
        "static analysis",
        true,
    ),
    (
        ChangeType::AuthorizationSensitive,
        "betterleaks",
        "betterleaks",
        "secrets",
        false,
    ),
    (ChangeType::DataSchema, "cargo-test", "cargo", "tests", true),
    (
        ChangeType::DataSchema,
        "cargo-mutants",
        "cargo-mutants",
        "mutation",
        false,
    ),
];

/// What this machine can actually run, as a value rather than a global.
///
/// Passed in so [`propose`] is a pure function of `(store, environment)` and a
/// test can state the environment instead of depending on whatever is installed
/// on the machine running it. [`ProfileEnvironment::from_machine`] is the one
/// place that looks at the machine, and it reads `PATH` and the filesystem —
/// no process is spawned and no network is touched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileEnvironment {
    pub tools_present: BTreeSet<String>,
    pub recipes_present: BTreeSet<String>,
}

impl ProfileEnvironment {
    /// Nothing installed, no settings files. The honest default for a test that
    /// is not about this machine.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Read `PATH` and the project's `.thruline/` directory.
    pub fn from_machine(project_root: Option<&std::path::Path>) -> Self {
        let mut tools_present = BTreeSet::new();
        let mut recipes_present = BTreeSet::new();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path).collect();
        for (_, id, tool, _, _) in PROFILE_TABLE {
            if dirs.iter().any(|dir| dir.join(tool).is_file()) {
                tools_present.insert((*tool).to_string());
            }
            if let Some(root) = project_root {
                if root.join(recipe_path(id)).is_file() {
                    recipes_present.insert((*id).to_string());
                }
            }
        }
        Self {
            tools_present,
            recipes_present,
        }
    }

    fn availability(&self, id: &str, tool: &str) -> Availability {
        if !self.tools_present.contains(tool) {
            return Availability::ToolAbsent {
                tool: tool.to_string(),
            };
        }
        if !self.recipes_present.contains(id) {
            return Availability::RecipeMissing {
                recipe: recipe_path(id),
            };
        }
        Availability::Ready
    }
}

/// One profile check's category, by adapter id — `tests`, `lint`, `behaviour`,
/// `static analysis`, `secrets`, `mutation`.
///
/// T-3-1 needs to know what class of work an accepted check is for, and
/// [`PROFILE_TABLE`] is the one place that says so. Reading it through a
/// function rather than copying the pairs into a second table is what stops the
/// two from drifting; `None` means the id is not one of ours, which is a fact
/// and not a default.
pub fn profile_check_category(id: &str) -> Option<&'static str> {
    PROFILE_TABLE
        .iter()
        .find(|(_, entry_id, _, _, _)| *entry_id == id)
        .map(|(_, _, _, category, _)| *category)
}

/// One accepted check, with what it is for and whether it can run here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptedCheck {
    pub id: String,
    pub tool: &'static str,
    pub category: &'static str,
    pub mandatory: bool,
    pub availability: Availability,
}

/// What an already-accepted check profile asks for, on this machine.
///
/// [`profile_for_paths`] answers "what would we suggest for these paths".
/// This answers the different question T-3-1 asks: the profile is already
/// accepted and written down, and the only open question is whether the tools
/// it names are here. An id nobody registered is skipped rather than invented.
pub fn accepted_check_availability(
    check_profile: &[String],
    environment: &ProfileEnvironment,
) -> Vec<AcceptedCheck> {
    let mut out: Vec<AcceptedCheck> = Vec::new();
    for id in check_profile {
        let Some((_, entry_id, tool, category, mandatory)) = PROFILE_TABLE
            .iter()
            .find(|(_, entry_id, _, _, _)| entry_id == id)
        else {
            continue;
        };
        if out.iter().any(|a| a.id == *entry_id) {
            continue;
        }
        out.push(AcceptedCheck {
            id: (*entry_id).to_string(),
            tool,
            category,
            mandatory: *mandatory,
            availability: environment.availability(entry_id, tool),
        });
    }
    out
}

/// The check profile this Change has accepted, at its standing scope version.
///
/// Empty when nothing has been accepted: a Change that has not said what counts
/// as done has not named a check either, and inventing one would be the product
/// deciding on a person's behalf.
pub fn accepted_check_profile(ledger: &Ledger, change_id: &str) -> Result<Vec<String>> {
    let row: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT r.check_profile_json
               FROM change_scope_revisions r
               JOIN change_scope_acceptances a
                 ON a.change_id=r.change_id AND a.scope_version=r.scope_version
              WHERE r.change_id=?1
              ORDER BY r.scope_version DESC LIMIT 1",
            rusqlite::params![change_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(row
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default())
}

/// Where a reviewed settings file for one adapter lives, relative to the
/// project root. The shape `spec/integrations/support.json` already states.
pub fn recipe_path(id: &str) -> String {
    format!(".thruline/{id}.json")
}

/// This machine, as seen from one Change's project root.
///
/// The one function a surface calls, so the CLI and the HTTP route cannot
/// disagree about what "available here" means.
pub fn machine_environment(ledger: &Ledger, change_id: &str) -> Result<ProfileEnvironment> {
    let project: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM changes WHERE id=?1",
            params![change_id],
            |row| row.get(0),
        )
        .optional()?;
    let bound = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?
    .and_then(|row| row.worktree_path.value().map(std::path::PathBuf::from));
    let root = match bound {
        Some(root) if root.is_dir() => Some(root),
        _ => project
            .as_deref()
            .map(|id| crate::workspace::established_project_root(ledger, id, None))
            .transpose()?
            .flatten(),
    };
    Ok(ProfileEnvironment::from_machine(root.as_deref()))
}

/// The suggested profile: what kind of work this is, which checks that asks
/// for, and the sentence that says a list of checks is not evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileSuggestion {
    pub change_types: Vec<ChangeType>,
    /// The copy-file slug for each kind, in the same order.
    pub change_type_slugs: Vec<&'static str>,
    /// The paths the kinds were read off, masked.
    pub paths_read: Vec<String>,
    pub checks: Vec<ProfileCheck>,
    /// `B5.10`, said out loud on every reading.
    pub not_evidence_slug: &'static str,
    /// The copy-file slug when no path in the record says what kind of work
    /// this is, `None` otherwise.
    pub unknown_slug: Option<&'static str>,
}

/// The profile one set of paths asks for, on one machine.
///
/// Public so a test can state its own paths and its own machine instead of
/// depending on what happens to be installed where it runs. [`propose`] is its
/// only other caller.
pub fn profile_for_paths(paths: &[String], environment: &ProfileEnvironment) -> ProfileSuggestion {
    let mut record = MaskRecord::default();
    suggest_profile(paths, environment, &mut record)
}

fn suggest_profile(
    paths: &[String],
    environment: &ProfileEnvironment,
    record: &mut MaskRecord,
) -> ProfileSuggestion {
    let mut types: BTreeSet<ChangeType> = BTreeSet::new();
    for path in paths {
        for kind in types_of_path(path) {
            types.insert(kind);
        }
    }
    let unknown_slug = if types.is_empty() {
        Some("profile_no_paths")
    } else {
        None
    };
    let change_types: Vec<ChangeType> = types.iter().copied().collect();

    // One row per adapter, carrying every kind that asked for it, so a check is
    // never listed twice and never loses the reason it is here.
    let mut by_id: BTreeMap<&'static str, ProfileCheck> = BTreeMap::new();
    for (kind, id, tool, category, mandatory) in PROFILE_TABLE {
        if !types.contains(kind) {
            continue;
        }
        let entry = by_id.entry(id).or_insert_with(|| {
            let availability = environment.availability(id, tool);
            ProfileCheck {
                id,
                tool,
                category,
                mandatory: *mandatory,
                for_types: Vec::new(),
                need_slug: if *mandatory {
                    "check_mandatory"
                } else {
                    "check_advisory"
                },
                availability_slug: availability.slug(),
                availability_name: availability.name(),
                availability,
            }
        });
        entry.for_types.push(*kind);
        // A check asked for by two kinds is needed if either kind needs it.
        // Weakening by union would be the silent weakening `B5.5` forbids.
        if *mandatory && !entry.mandatory {
            entry.mandatory = true;
            entry.need_slug = "check_mandatory";
        }
    }
    ProfileSuggestion {
        change_types,
        change_type_slugs: types.iter().map(|k| k.slug()).collect(),
        paths_read: mask_list(paths.to_vec(), record),
        checks: by_id.into_values().collect(),
        not_evidence_slug: "profile_is_not_evidence",
        unknown_slug,
    }
}

// ---------------------------------------------------------------------------
// The diff against what was already accepted
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// What this Change covers (T-3-2b)
// ---------------------------------------------------------------------------

/// How many folders one proposal may cover.
///
/// Bounded for the same reason the version series is: a list a person cannot
/// read is a list nobody agreed to. What is beyond the bound is SAID, with the
/// count, never silently dropped.
pub const MAX_SCOPE_PATHS: usize = 25;

/// The folders a set of changed files live in, as allowed-scope entries.
///
/// **It invents no path.** Every entry is the directory of a file the record
/// says one confirmed session changed, spelled as the subtree
/// [`crate::change_link::pattern_matches`] already understands; a file the
/// record holds at the top of the project becomes that exact path, because
/// `/**` on nothing would cover the whole project. An entry already covered by
/// a shorter one is dropped, so accepting this list twice cannot grow it.
///
/// Pure: no store, no clock, no machine. The same changed paths always give the
/// same list, in the same order.
pub fn scope_for_paths(paths: &[String]) -> Vec<String> {
    let mut entries: BTreeSet<String> = BTreeSet::new();
    for path in paths {
        let trimmed = path.trim().trim_start_matches("./");
        if trimmed.is_empty()
            || trimmed.starts_with('/')
            || trimmed
                .split('/')
                .any(|part| part == ".." || part.is_empty())
        {
            continue;
        }
        match trimmed.rsplit_once('/') {
            Some((dir, _)) if !dir.is_empty() => entries.insert(format!("{dir}/**")),
            // A file the record holds at the top of the project, or one the
            // record holds as an absolute path with no project root to cut it
            // against: named exactly, never widened into a folder.
            _ => entries.insert(trimmed.to_string()),
        };
    }
    let all: Vec<String> = entries.into_iter().collect();
    all.iter()
        .filter(|entry| {
            !all.iter().any(|other| {
                other.as_str() != entry.as_str()
                    && matches!(
                        crate::change_link::pattern_matches(other, entry.trim_end_matches("/**")),
                        crate::change_link::PatternVerdict::Matched
                    )
            })
        })
        .cloned()
        .collect()
}

/// What this Change covers, proposed from the work already confirmed as part
/// of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposedScope {
    /// The folders, in the order they are shown.
    pub paths: Vec<String>,
    /// How many files the folders were read off. A typed count of stored rows.
    pub from_files: u64,
    /// How many folders were left off by the bound. Zero is the ordinary case
    /// and is still served, because a count that only appears when it is bad is
    /// a count nobody trusts.
    pub not_shown: u64,
    pub bound: u64,
    /// Why these folders, in Sam's words.
    pub reason_slug: &'static str,
    /// The copy-file slug when the record names no file at all.
    pub empty_slug: Option<&'static str>,
    /// The copy-file slug when the bound left folders off this list.
    pub over_bound_slug: Option<&'static str>,
}

/// One accepted criterion beside the proposal that answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopeComparison {
    pub accepted: String,
    /// The proposal's number, when one answers this accepted line.
    pub proposed_number: Option<u32>,
    pub proposed: Option<String>,
    pub sentence_slug: &'static str,
}

/// What a fresh reading says about a Change that already has an accepted list.
///
/// The point of this type is `B5.5`: a second proposal round must be **shown**
/// beside the accepted version, with what it drops and what it softens named,
/// and it must change nothing. Nothing in this module writes without
/// [`accept`], and [`accept`] requires an actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopeDiff {
    pub accepted_version: u32,
    /// Accepted lines nothing in this reading proposes. They stay accepted.
    pub removed: Vec<ScopeComparison>,
    /// Accepted lines this reading proposes something *weaker* than.
    pub weakened: Vec<ScopeComparison>,
    /// Accepted lines this reading says differently, neither stronger nor
    /// weaker by the two mechanical tests.
    pub restated: Vec<ScopeComparison>,
    /// Proposal numbers this reading has that the accepted version does not.
    pub added: Vec<u32>,
    /// The copy-file slug for what a new round does to the accepted version:
    /// nothing.
    pub nothing_changed_slug: &'static str,
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn word_set(text: &str) -> BTreeSet<String> {
    normalized(text).split(' ').map(|w| w.to_string()).collect()
}

/// Compare a fresh proposal set against one accepted version.
///
/// Two mechanical tests for "weaker", and only two, because a third that needed
/// judgement would be a judgement nobody could check:
///
/// 1. the accepted line says "not" about its outcome and the new one does not —
///    a dropped negation reverses a requirement;
/// 2. every word of the new line is already in the accepted line and the
///    accepted line has more — the new line says strictly less.
pub fn diff_against_accepted(
    accepted_version: u32,
    accepted: &[String],
    proposed: &[ProposedCriterion],
) -> ScopeDiff {
    let mut removed = Vec::new();
    let mut weakened = Vec::new();
    let mut restated = Vec::new();
    let mut answered: BTreeSet<u32> = BTreeSet::new();

    for line in accepted {
        let shape = observable_shape(line).ok();
        let key = shape.as_ref().map(|s| s.subject_key());
        let partner = key.as_ref().and_then(|key| {
            proposed
                .iter()
                .find(|candidate| &candidate.subject_key == key)
        });
        let Some(partner) = partner else {
            removed.push(ScopeComparison {
                accepted: line.clone(),
                proposed_number: None,
                proposed: None,
                sentence_slug: "reproposal_removed",
            });
            continue;
        };
        answered.insert(partner.number);
        if normalized(line) == normalized(&partner.text) {
            continue;
        }
        let lost_negation = shape.as_ref().is_some_and(|s| s.outcome_is_negated())
            && !partner.shape.outcome_is_negated();
        let accepted_words = word_set(line);
        let proposed_words = word_set(&partner.text);
        // `is_subset` plus a length test, deliberately, and not `<`: `BTreeSet`
        // orders lexicographically, so `proposed < accepted` is not "proposed
        // says less" and quietly answered `false` for the exact case this test
        // exists for.
        let says_less = proposed_words.is_subset(&accepted_words)
            && proposed_words.len() < accepted_words.len();
        let comparison = ScopeComparison {
            accepted: line.clone(),
            proposed_number: Some(partner.number),
            proposed: Some(partner.text.clone()),
            sentence_slug: if lost_negation || says_less {
                "reproposal_weakened"
            } else {
                "reproposal_restated"
            },
        };
        if lost_negation || says_less {
            weakened.push(comparison);
        } else {
            restated.push(comparison);
        }
    }

    ScopeDiff {
        accepted_version,
        removed,
        weakened,
        restated,
        added: proposed
            .iter()
            .map(|p| p.number)
            .filter(|n| !answered.contains(n))
            .collect(),
        nothing_changed_slug: "reproposal_changes_nothing",
    }
}

// ---------------------------------------------------------------------------
// The accepted version, and who wrote each line of it
// ---------------------------------------------------------------------------

/// Whose words an accepted line is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Attribution {
    /// Word for word one of the proposals this store derives.
    AsProposed,
    /// Not any proposal's words. The accepting person's, then — never ours.
    YourWords,
}

impl Attribution {
    pub fn slug(self) -> &'static str {
        match self {
            Self::AsProposed => "criterion_as_proposed",
            Self::YourWords => "criterion_your_words",
        }
    }
}

/// One line of the accepted list, with whose words it is in and whether it can
/// still be checked as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptedCriterion {
    pub text: String,
    pub attribution: Attribution,
    pub attribution_slug: &'static str,
    /// Present when the line, as written, cannot be cut into the three parts.
    /// Shown beside it, never used to refuse or to silently drop it.
    pub not_observable_slug: Option<&'static str>,
}

/// The standing accepted version as this view serves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptedEcho {
    pub version: u32,
    pub criteria: Vec<AcceptedCriterion>,
    pub check_profile: Vec<String>,
    /// What the standing version says this Change covers. Empty is a state a
    /// surface must show as such: nothing is counted under a Change that covers
    /// nothing.
    pub allowed_scope: Vec<String>,
    /// Whose list it is: the folders this store proposed, or the person's own.
    pub scope_attribution: Attribution,
    pub scope_attribution_slug: &'static str,
    /// `None` when a version exists that nobody has agreed to. Shown as
    /// missing, never as agreed.
    pub acceptance: Option<change::AcceptanceView>,
    /// The copy-file slug for "accepting is not approving", served with every
    /// acceptance so a surface cannot leave it off.
    pub not_an_approval_slug: &'static str,
}

/// The request, retained beside the proposals it produced (`B5.3`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetainedRequest {
    pub title: String,
    pub purpose: String,
    pub prompt_event_id: Option<String>,
    pub requester: String,
}

/// Everything one reading of one Change proposes, plus everything it could not
/// propose, plus what is already accepted and how this reading differs from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposalView {
    pub schema_version: u32,
    pub change_id: String,
    /// `B5.3` — the original request travels with the proposals it produced.
    pub request: RetainedRequest,
    /// The sessions someone confirmed belong to this Change, in id order.
    pub confirmed_sessions: Vec<String>,
    pub criteria: Vec<ProposedCriterion>,
    pub notes: Vec<KeptNote>,
    pub profile: ProfileSuggestion,
    /// What this Change covers, proposed (T-3-2b). Accepted alongside the
    /// criteria, in one act, because a Change with criteria and no covered
    /// folders is a Change the scope check refuses every piece of work under.
    pub scope: ProposedScope,
    pub accepted: Option<AcceptedEcho>,
    pub diff: Option<ScopeDiff>,
    pub versions_used: u32,
    pub versions_allowed: u32,
    /// The copy-file slug for the bound, always present so a person can see it
    /// before they hit it.
    pub versions_slug: &'static str,
    /// The copy-file slug when there is nothing to propose, `None` otherwise.
    pub empty_slug: Option<&'static str>,
    /// The copy-file slug when nothing has been accepted yet.
    pub not_accepted_slug: Option<&'static str>,
    pub masking: crate::redact::Treatment,
}

// ---------------------------------------------------------------------------
// Reading the requests
// ---------------------------------------------------------------------------

/// One span of a request that might be a sentence, and the two facts about how
/// it ended that decide whether it may be proposed as a criterion.
struct Span {
    offset: usize,
    text: String,
    /// The span ended on a full stop, `!`, `?` or `;` that this splitter
    /// treated as a boundary — not on a line break and not on the end of the
    /// request.
    stopped: bool,
    /// The span began at the start of a line (leading spaces and list markers
    /// do not count), rather than in the middle of one.
    line_start: bool,
    /// The span is shorter than [`MIN_SENTENCE_CHARS`]. It is still a span —
    /// kept as a note, never dropped (`QUAL-F11-LAUNCH-R5`).
    too_short: bool,
}

/// Is a `.` at `index` the end of a sentence, or a dot inside somebody's
/// writing?
///
/// Three kinds of dot are not a full stop, and `T-2-8-F1` is what each one cost
/// when it was treated as one — the product's first true statement about a
/// person's own project was cut into "…and asking." and "rs currently returns
/// OutboxError::RetryNotImplemented":
///
/// * **inside a token**: `src/lib.rs`, `v1.2.3` — the dot has a non-space on
///   both sides;
/// * **inside an abbreviation**: `e.g.` — the run of non-space characters
///   before the dot already contains one;
/// * **inside backticks**: `` `Outbox::request_retry` `` and anything else a
///   person quoted as code on one line.
///
/// Nothing here is a judgement about English. Each rule is a fact about the
/// characters either side of the dot, so the same request always cuts the same
/// way.
fn ends_a_sentence(bytes: &[u8], index: usize) -> bool {
    let next = bytes.get(index + 1).copied();
    // A dot with a non-space after it is inside a token: `src/lib.rs`.
    match next {
        None => {}
        Some(b) if b.is_ascii_whitespace() => {}
        Some(_) => return false,
    }
    // A dot whose own word already holds one is an abbreviation: `e.g.`.
    let word_start = bytes[..index]
        .iter()
        .rposition(|b| b.is_ascii_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    !bytes[word_start..index].contains(&b'.')
}

/// The byte ranges of text inside a matched pair of backticks, one line at a
/// time.
///
/// Per line deliberately: an unclosed backtick then protects the rest of its
/// own line and nothing more, instead of welding a whole request into one
/// "sentence". A person who opens a backtick and never closes it should lose
/// one line to it, not their entire request.
fn backtick_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut line_from = 0usize;
    for line in text.split_inclusive('\n') {
        let ticks: Vec<usize> = line
            .bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'`')
            .map(|(i, _)| i)
            .collect();
        for pair in ticks.chunks_exact(2) {
            spans.push((line_from + pair[0], line_from + pair[1]));
        }
        line_from += line.len();
    }
    spans
}

/// Is this line break the end of a sentence, or a soft wrap inside one?
///
/// A hard-wrapped paragraph — which is how every agent prompt this product has
/// met is written — puts a newline in the middle of a sentence. Treating it as
/// a boundary is the other half of `T-2-8-F1`: it is what turned one request
/// into "…in the `retry_jobs` table, and asking" and "twice for the same
/// delivery while a job is still pending…", two fragments offered for one-click
/// acceptance.
///
/// A break ends a sentence when the next line starts something: a blank line, a
/// list marker, or a capital. A next line that starts in lower case is the rest
/// of the line above.
fn breaks_a_sentence(text: &str, index: usize) -> bool {
    let rest = &text[index + 1..];
    let Some(next) = rest.chars().find(|c| !matches!(c, ' ' | '\t')) else {
        return true; // nothing follows; the request ends here
    };
    if next == '\n' || next == '\r' {
        return true; // a blank line: a new paragraph
    }
    if matches!(next, '-' | '*' | '•' | '#' | '>') || next.is_ascii_digit() {
        return true; // a list marker or a numbered item
    }
    next.is_uppercase()
}

/// Is this semicolon the end of a sentence, or the join between two clauses of
/// one?
///
/// The line-break rule, applied to what follows it: a semicolon ends a sentence
/// before a capital, a list marker, a blank line or the end of the request, and
/// never before the rest of its own sentence (`QUAL-F11`: "… is shown as
/// failed; the service saying 'sent' does not make it sent." is one sentence).
/// A semicolon at the end of a line asks the same question of that line break,
/// so a hard wrap right after it cuts nothing a wrap anywhere else would not.
fn semicolon_ends_a_sentence(text: &str, index: usize) -> bool {
    let after = index + 1;
    match text[after..]
        .char_indices()
        .find(|(_, c)| !matches!(c, ' ' | '\t'))
    {
        Some((offset, '\r' | '\n')) => match text[after + offset..].find('\n') {
            Some(newline) => breaks_a_sentence(text, after + offset + newline),
            None => true,
        },
        _ => breaks_a_sentence(text, index),
    }
}

/// The quotations a person wrote in one stretch of text, whether any mark was
/// left unmatched (`QUAL-F11-LAUNCH-R2`, `-R3`), and where the marks stop
/// having one reading (`QUAL-F11-LAUNCH-R4`).
struct Quotations {
    /// Byte ranges from an opening mark to its closing mark. Nothing between
    /// them ends a span: "shows 'delivered; Retry disabled' when …" is one
    /// sentence, whatever the `;` is followed by.
    pairs: Vec<(usize, usize)>,
    /// No reading of the marks leaves every quotation closed and every mark
    /// matched: a quotation was opened and never closed, a quote mark closed
    /// nothing, or a backtick has no partner. Text like that is a piece of a
    /// sentence, so it is never proposed.
    unclosed: bool,
    /// The byte offset of the first single mark with two readings. Nothing
    /// after it, to the end of the stretch, ends a span.
    held_from: Option<usize>,
}

/// Words a leading single mark shortens rather than quotes: 'til, ’tis,
/// 'twas, 'em, 'cause, 'bout, 'round, 'n, 'nuff, 'cos. A mark before a digit
/// (’90s) is one too.
const ELISIONS: &[&str] = &[
    "bout", "cause", "cos", "em", "n", "nuff", "round", "til", "tis", "twas",
];

/// What one single mark does in one reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SingleRole {
    Open,
    /// Closes the quotation the mark at this byte offset opened.
    Close(usize),
    Apostrophe,
}

/// One single quote mark that is not inside a word, and the facts about the
/// characters either side of it that decide what it may be.
struct SingleMark {
    at: usize,
    /// It starts a word: nothing, a space, an opening bracket or a double mark
    /// before it, and a non-space after it.
    opens: bool,
    /// It ends a word: a non-space before it, and no letter or digit after.
    closes: bool,
    /// It ends a word straight after an `s`: the drivers' report.
    possessive: bool,
    /// It starts a word that is an elision ([`ELISIONS`], or a digit).
    elision: bool,
}

/// Every role `mark` may take while `open` is the open single quotation (by
/// the byte offset of its opening mark), each with the quotation open after
/// it. Empty when the mark closes nothing.
fn single_roles(mark: &SingleMark, open: Option<usize>) -> Vec<(SingleRole, Option<usize>)> {
    match open {
        Some(from) if mark.closes => {
            let mut roles = vec![(SingleRole::Close(from), None)];
            if mark.possessive {
                roles.push((SingleRole::Apostrophe, open));
            }
            roles
        }
        Some(_) => vec![(SingleRole::Apostrophe, open)],
        None if mark.opens => {
            let mut roles = vec![(SingleRole::Open, Some(mark.at))];
            if mark.elision {
                roles.push((SingleRole::Apostrophe, None));
            }
            roles
        }
        None if mark.possessive => vec![(SingleRole::Apostrophe, None)],
        None => Vec::new(),
    }
}

/// Read the single marks of one stretch: the pairs, whether they can all be
/// matched, and the first mark with two readings.
///
/// Every reading is walked at once, as the set of quotations that could be
/// open after each mark; a reading is valid when it matches every mark and
/// closes every quotation. When one is valid, a mark has two readings when
/// two valid readings give it different roles. When none is, the marks are
/// read left to right, as round 3 read them, and a mark that could have been
/// read either way is still the point cutting stops.
fn read_singles(marks: &[SingleMark]) -> (Vec<(usize, usize)>, bool, Option<usize>) {
    type States = std::collections::BTreeSet<Option<usize>>;
    let mut reachable: Vec<States> = vec![States::from([None])];
    for (i, mark) in marks.iter().enumerate() {
        let next: States = reachable[i]
            .iter()
            .flat_map(|state| single_roles(mark, *state).into_iter().map(|(_, s)| s))
            .collect();
        reachable.push(next);
    }
    // Walk back from the one valid end: nothing open.
    let mut live: Vec<States> = vec![States::new(); marks.len() + 1];
    if reachable[marks.len()].contains(&None) {
        live[marks.len()].insert(None);
    }
    for i in (0..marks.len()).rev() {
        let after = live[i + 1].clone();
        live[i] = reachable[i]
            .iter()
            .filter(|state| {
                single_roles(&marks[i], **state)
                    .iter()
                    .any(|(_, s)| after.contains(s))
            })
            .copied()
            .collect();
    }

    let mut pairs = Vec::new();
    let mut held_from = None;
    if live[0].contains(&None) {
        let mut state: Option<usize> = None;
        for (i, mark) in marks.iter().enumerate() {
            let roles: std::collections::BTreeSet<SingleRole> = live[i]
                .iter()
                .flat_map(|s| single_roles(mark, *s))
                .filter(|(_, after)| live[i + 1].contains(after))
                .map(|(role, _)| role)
                .collect();
            if roles.len() > 1 && held_from.is_none() {
                held_from = Some(mark.at);
            }
            if let Some((role, after)) = single_roles(mark, state)
                .into_iter()
                .find(|(_, after)| live[i + 1].contains(after))
            {
                if let SingleRole::Close(from) = role {
                    pairs.push((from, mark.at));
                }
                state = after;
            }
        }
        return (pairs, false, held_from);
    }

    let mut state: Option<usize> = None;
    let mut stray = false;
    for mark in marks {
        let roles = single_roles(mark, state);
        if roles.len() > 1 && held_from.is_none() {
            held_from = Some(mark.at);
        }
        match roles.first() {
            Some((role, after)) => {
                if let SingleRole::Close(from) = role {
                    pairs.push((*from, mark.at));
                }
                state = *after;
            }
            None => stray = true,
        }
    }
    (pairs, stray || state.is_some(), held_from)
}

/// Find the quotations in `text`, and a count of backticks (whose ranges
/// [`backtick_spans`] already protects).
///
/// The rule, each part a fact about the characters either side of a mark, so
/// the same request always reads the same way:
///
/// - There are two families of quote mark, single (`'` `‘` `’`) and double
///   (`"` `“` `”`). Within a family, any mark closes a quotation that any
///   mark opened — an editor that smart-quotes one end of a pair and not the
///   other is the ordinary case, not an attack. Families never pair with
///   each other (`QUAL-F11-LAUNCH-R3`).
/// - A double mark opens when nothing of its family is open and it starts a
///   word, and closes the open quotation when it ends one. A straight `"`
///   closes whatever double quotation is open, and opens one otherwise.
///   Double marks have one reading each.
/// - A single mark with a letter or digit on both sides — don’t, it's, the
///   service’s — is an apostrophe and not a quote mark.
/// - **A single mark may have two readings** (`QUAL-F11-LAUNCH-R4`). While a
///   single quotation is open, a mark that ends a word closes it — or, after
///   an `s`, may be a plural possessive (the drivers' report) that leaves it
///   open. While none is open, a mark that starts a word opens one — or,
///   before an elision ('til, ’90s), may be an apostrophe. With none open, a
///   mark after an `s` that ends a word is a possessive; any other mark
///   closes nothing. [`read_singles`] keeps only the readings that match
///   every mark, and names the first mark those readings disagree about.
/// - A quotation still open at the end, or a mark that matches nothing in
///   every reading, makes the text `unclosed`.
fn quotations_in(text: &str) -> Quotations {
    let mut pairs = Vec::new();
    let mut double: Option<usize> = None;
    let mut singles: Vec<SingleMark> = Vec::new();
    let mut backticks = 0usize;
    let mut stray = false;
    let mut prev: Option<char> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        let next = chars.peek().map(|(_, c)| *c);
        let opens = prev
            .is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '"' | '“' | '”'))
            && next.is_some_and(|c| !c.is_whitespace());
        let closes =
            prev.is_some_and(|c| !c.is_whitespace()) && next.is_none_or(|c| !c.is_alphanumeric());
        match ch {
            '"' => match double.take() {
                Some(from) => pairs.push((from, index)),
                None => double = Some(index),
            },
            '“' | '”' => match double {
                Some(from) if closes => {
                    pairs.push((from, index));
                    double = None;
                }
                Some(_) => {}
                None if opens => double = Some(index),
                None => stray = true,
            },
            '\'' | '‘' | '’' => {
                let inside_a_word = prev.is_some_and(char::is_alphanumeric)
                    && next.is_some_and(char::is_alphanumeric);
                if !inside_a_word {
                    let word: String = text[index + ch.len_utf8()..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric())
                        .collect();
                    let elision = opens
                        && (word.starts_with(|c: char| c.is_ascii_digit())
                            || ELISIONS.contains(&word.to_lowercase().as_str()));
                    singles.push(SingleMark {
                        at: index,
                        opens,
                        closes,
                        possessive: closes && matches!(prev, Some('s' | 'S')),
                        elision,
                    });
                }
            }
            '`' => backticks += 1,
            _ => {}
        }
        prev = Some(ch);
    }
    let (single_pairs, single_unmatched, held_from) = read_singles(&singles);
    pairs.extend(single_pairs);
    let unclosed = stray || single_unmatched || backticks % 2 == 1 || double.is_some();
    Quotations {
        pairs,
        unclosed,
        held_from,
    }
}

/// A byte range `(from, to)` of a request.
type ByteRange = (usize, usize);

/// The quotation ranges of a whole request, found one stretch at a time, and
/// the ranges in which nothing is cut because a mark before them has two
/// readings.
///
/// A stretch runs from one sentence-ending line break to the next, so a
/// quotation may cross a soft wrap but never a paragraph, a list item or a
/// line that starts a new sentence: a quote someone forgot to close costs the
/// stretch it is in, and never welds two requirements together. Because no
/// stretch holds a line break that ends a sentence, every span-ending rule —
/// `.`, `!`, `?`, `;` and the line break — is held back inside a quotation,
/// and every one but the line break is held back after a mark with two
/// readings (`QUAL-F11-LAUNCH-R4`), unless it ends the stretch and so cuts
/// nothing off ([`ends_its_stretch`]).
fn quotation_spans(text: &str) -> (Vec<ByteRange>, Vec<ByteRange>) {
    let mut spans = Vec::new();
    let mut held = Vec::new();
    let mut from = 0usize;
    let mut stretch = |from: usize, to: usize, spans: &mut Vec<(usize, usize)>| {
        let found = quotations_in(&text[from..to]);
        spans.extend(found.pairs.into_iter().map(|(a, b)| (from + a, from + b)));
        if let Some(at) = found.held_from {
            held.push((from + at, to));
        }
    };
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' && breaks_a_sentence(text, index) {
            stretch(from, index, &mut spans);
            from = index + 1;
        }
    }
    stretch(from, text.len(), &mut spans);
    (spans, held)
}

/// Is nothing but spaces left in this stretch after `index` — the end of the
/// request, or a line break that starts a new sentence?
fn ends_its_stretch(text: &str, index: usize) -> bool {
    let after = index + 1;
    match text[after..]
        .char_indices()
        .find(|(_, c)| !matches!(c, ' ' | '\t'))
    {
        None => true,
        Some((offset, '\r' | '\n')) => match text[after + offset..].find('\n') {
            Some(newline) => breaks_a_sentence(text, after + offset + newline),
            None => true,
        },
        Some(_) => false,
    }
}

/// Split one request into the sentences a criterion could be cut out of.
///
/// Bullets and paragraph breaks are boundaries as much as full stops are: a
/// request written as a list is the most checkable kind there is, and treating
/// it as one sentence would make every item a paragraph. A soft wrap inside a
/// paragraph is not a boundary, and neither is a dot inside a path, a version
/// number, an abbreviation or a pair of backticks.
fn sentences_of(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    let mut protected = backtick_spans(text);
    let (quoted, held) = quotation_spans(text);
    protected.extend(quoted);
    // Inside a quotation, or after a quote mark with two readings
    // (`QUAL-F11-LAUNCH-R4`): no `.`, `!`, `?` or `;` ends a span there —
    // except the one that ends the stretch, which leaves nothing behind it to
    // cut off, and is the person's own stop.
    let inside = |i: usize| {
        protected.iter().any(|(from, to)| i > *from && i < *to)
            || (held.iter().any(|(from, to)| i > *from && i < *to) && !ends_its_stretch(text, i))
    };
    let mut index = 0usize;
    while index < bytes.len() {
        let ch = bytes[index];
        let here = index;
        index += 1;
        let stopped = match ch {
            b'\n' => {
                if !breaks_a_sentence(text, here) {
                    continue;
                }
                false
            }
            b'.' => {
                if inside(here) || !ends_a_sentence(bytes, here) {
                    continue;
                }
                true
            }
            b'!' | b'?' => {
                if inside(here) {
                    continue;
                }
                true
            }
            b';' => {
                if inside(here) || !semicolon_ends_a_sentence(text, here) {
                    continue;
                }
                true
            }
            _ => continue,
        };
        push_sentence(text, start, index, stopped, &mut out);
        start = index;
    }
    if start < text.len() {
        push_sentence(text, start, text.len(), false, &mut out);
    }
    out
}

/// A mark that opens or closes quoted text: the two quote families and the
/// backtick.
fn is_quote_mark(c: char) -> bool {
    matches!(c, '\'' | '‘' | '’' | '"' | '“' | '”' | '`')
}

fn push_sentence(text: &str, from: usize, to: usize, stopped: bool, out: &mut Vec<Span>) {
    let raw = &text[from..to];
    // Leading list markers are not part of the sentence, and dropping them
    // keeps the subject test from failing on a bullet. A quote mark or a
    // backtick is the person's own writing, and stays (`QUAL-F11-LAUNCH-R4`:
    // "'Saved' is what the banner shows" lost its opening mark).
    let trimmed_start = raw
        .char_indices()
        .find(|(_, c)| c.is_alphanumeric() || is_quote_mark(*c))
        .map(|(i, _)| i)
        .unwrap_or(raw.len());
    // The stored sentence is the CORE: no trailing terminator, so a note and a
    // criterion quote the same span the same way and a span length means one
    // thing.
    let body = core_of(&raw[trimmed_start..]);
    // A span with nothing the person wrote in it — only spaces, list markers
    // or stops — has no words to keep. Anything else is kept, however short
    // (`QUAL-F11-LAUNCH-R5`: "Retry now." used to vanish here).
    if body.is_empty() {
        return;
    }
    let too_short = body.chars().count() < MIN_SENTENCE_CHARS;
    // Did this span begin a line? Everything between the last line break and
    // the first word is whitespace and list markers, which `trimmed_start`
    // has already stepped over.
    let line_start = text[..from].chars().next_back().is_none_or(|c| c == '\n')
        || text[from..from + trimmed_start].contains('\n');
    out.push(Span {
        offset: from + trimmed_start,
        text: body.to_string(),
        stopped,
        line_start,
        too_short,
    });
}

/// May this span be offered as something that counts as done?
///
/// A criterion is a whole sentence about behaviour. A span that starts in the
/// middle of a line in lower case, or that trails off without a stop, is a
/// piece of one — and `T-2-8-F1` found three such pieces on the first screen
/// the product ever shows about a person's own project, two of them offered for
/// one-click acceptance ("asking for a delivery to be tried again enqueues one
/// job in the `retry_jobs` table, and asking.").
///
/// The splitter above should no longer produce those. This is the second
/// answer to the same question, because the first one is a set of rules about
/// characters and this one is a property of the result: whatever the splitter
/// does, nothing that does not read as a whole sentence is proposed. A
/// fragment is still kept — as a note, with its reason — because dropping it
/// would hide part of what somebody wrote (Rule 12).
fn is_whole_sentence(span: &Span) -> bool {
    let starts_with_capital = span
        .text
        .chars()
        .find(|c| !is_quote_mark(*c))
        .is_some_and(|c| c.is_uppercase() || c.is_numeric());
    // A span that opens a quotation and never closes it is half of what
    // somebody wrote, however it starts and ends (`QUAL-F11-LAUNCH-R2`).
    starts_with_capital && (span.stopped || span.line_start) && !quotations_in(&span.text).unclosed
}

/// The first thing a person wrote in one session, masked.
///
/// The same reading `candidates::derive` takes — the earliest `user_msg` by
/// `seq` — because a second definition of "the opening request" would be a
/// second answer to the same question.
fn session_opening(
    ledger: &Ledger,
    session_id: &str,
    record: &mut MaskRecord,
) -> Result<Option<(String, String)>> {
    let row: Option<(String, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT id, text FROM events
              WHERE session_id = ?1 AND kind = 'user_msg' ORDER BY seq LIMIT 1",
            params![session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((event_id, text)) = row else {
        return Ok(None);
    };
    let raw = text.unwrap_or_default();
    if raw.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some((event_id, mask_text(&raw, record))))
}

fn event_text(ledger: &Ledger, event_id: &str, record: &mut MaskRecord) -> Result<Option<String>> {
    let text: Option<Option<String>> = ledger
        .conn()
        .query_row(
            "SELECT text FROM events WHERE id = ?1",
            params![event_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(Some(raw)) = text else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(mask_text(&raw, record)))
}

/// Every session a person has confirmed belongs to this Change, in id order.
///
/// Confirmed, not suggested and not rejected: [`change_link::standing_link`] is
/// the only reader of that fact, and this calls it rather than re-deriving it.
pub fn confirmed_sessions(ledger: &Ledger, change_id: &str) -> Result<Vec<String>> {
    // The association table is not named in this module, deliberately: it is
    // read in three and only three (`ledger.rs` defines it, `change.rs` writes
    // it, `change_link.rs` reasons about it), and a fourth reader would be a
    // second idea of what a link is. T-2-2's census caught the first draft of
    // this function doing exactly that, and the repair was to ask rather than to
    // widen the census.
    let ids = change_link::linked_sessions(ledger, change_id)?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(standing) = change_link::standing_link(ledger, change_id, &id)? {
            if standing.provenance == AssociationProvenance::Confirmed {
                out.push(id);
            }
        }
    }
    Ok(out)
}

fn standing_scope(ledger: &Ledger, change_id: &str) -> Result<Option<(u32, AcceptedScope)>> {
    type Row = (u32, String, String, String, String, String);
    let row: Option<Row> = ledger
        .conn()
        .query_row(
            "SELECT scope_version, criteria_json, exclusions_json, allowed_scope_json,
                    constraints_json, check_profile_json
               FROM change_scope_revisions WHERE change_id = ?1
              ORDER BY scope_version DESC LIMIT 1",
            params![change_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    let Some((version, criteria, exclusions, allowed, constraints, profile)) = row else {
        return Ok(None);
    };
    let list = |raw: &str| serde_json::from_str::<Vec<String>>(raw).unwrap_or_default();
    Ok(Some((
        version,
        AcceptedScope {
            criteria: list(&criteria),
            exclusions: list(&exclusions),
            allowed_scope: list(&allowed),
            constraints: list(&constraints),
            check_profile: list(&profile),
        },
    )))
}

fn version_count(ledger: &Ledger, change_id: &str) -> Result<u32> {
    let count: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM change_scope_revisions WHERE change_id = ?1",
        params![change_id],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u32)
}

fn read_acceptance(
    ledger: &Ledger,
    change_id: &str,
    version: u32,
    record: &mut MaskRecord,
) -> Result<Option<change::AcceptanceView>> {
    let row: Option<(String, String, String)> = ledger
        .conn()
        .query_row(
            "SELECT accepted_by, accepted_at, rationale FROM change_scope_acceptances
              WHERE change_id = ?1 AND scope_version = ?2",
            params![change_id, version],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    Ok(
        row.map(|(actor, accepted_at, rationale)| change::AcceptanceView {
            actor: mask_text(&actor, record),
            accepted_at,
            rationale: mask_text(&rationale, record),
        }),
    )
}

// ---------------------------------------------------------------------------
// The proposer
// ---------------------------------------------------------------------------

/// Propose criteria and a check profile for one Change. **Reads only.**
///
/// Determinism is the contract, and it is structural: every input is a stored
/// row or the `environment` the caller passes, the candidate order is
/// `(origin, session id, position in the text)`, and there is no clock and no
/// randomness anywhere in it. `crates/core/tests/change_propose.rs` replays it.
pub fn propose(
    ledger: &Ledger,
    change_id: &str,
    environment: &ProfileEnvironment,
) -> Result<ProposalView> {
    type Head = (String, String, Option<String>, String);
    let head: Option<Head> = ledger
        .conn()
        .query_row(
            "SELECT title, purpose, request_event_id, requester FROM changes WHERE id = ?1",
            params![change_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((title, purpose, prompt_event_id, requester)) = head else {
        return Err(CoreError::Config(format!(
            "no change with id {change_id:?}"
        )));
    };

    let mut record = MaskRecord::default();
    let request = RetainedRequest {
        title: mask_text(&title, &mut record),
        purpose: mask_text(&purpose, &mut record),
        prompt_event_id: prompt_event_id.clone(),
        requester: mask_text(&requester, &mut record),
    };

    // Every request this Change can be read out of, in a stated order.
    let mut requests: Vec<(AskOrigin, Option<String>, Option<String>, String)> = Vec::new();
    if !request.purpose.trim().is_empty() {
        requests.push((
            AskOrigin::ChangePurpose,
            None,
            None,
            request.purpose.clone(),
        ));
    }
    if let Some(event_id) = &prompt_event_id {
        if let Some(text) = event_text(ledger, event_id, &mut record)? {
            requests.push((
                AskOrigin::OriginalPrompt,
                None,
                Some(event_id.clone()),
                text,
            ));
        }
    }
    let sessions = confirmed_sessions(ledger, change_id)?;
    for session_id in &sessions {
        if let Some((event_id, text)) = session_opening(ledger, session_id, &mut record)? {
            requests.push((
                AskOrigin::SessionOpening,
                Some(session_id.clone()),
                Some(event_id),
                text,
            ));
        }
    }
    requests.sort_by_key(|(origin, session, _, _)| (origin.order(), session.clone()));
    // One request read twice is one request. The commonest real configuration
    // does exactly that: a Change created from a session's opening prompt
    // points `request_event_id` at the very event that session opens with.
    // The second reading of the same event is not another request, so it is
    // read once, under the first origin it was reached by. Nothing else is
    // ever skipped (`QUAL-F11-LAUNCH-R6`).
    let mut read_events: BTreeSet<String> = BTreeSet::new();
    requests.retain(|(_, _, event_id, _)| match event_id {
        Some(event) => read_events.insert(event.clone()),
        None => true,
    });

    let mut candidates: Vec<Candidate> = Vec::new();
    for (origin, session_id, event_id, text) in &requests {
        let key = match (session_id, event_id) {
            (_, Some(event)) => event.clone(),
            (Some(session), None) => session.clone(),
            (None, None) => change_id.to_string(),
        };
        for span in sentences_of(text) {
            candidates.push(Candidate {
                source: AskSource {
                    ask_id: ask_id(*origin, &key),
                    origin: *origin,
                    session_id: session_id.clone(),
                    event_id: event_id.clone(),
                    offset: span.offset,
                    length: span.text.len(),
                },
                whole: is_whole_sentence(&span),
                too_short: span.too_short,
                text: span.text,
            });
        }
    }

    let mut criteria: Vec<ProposedCriterion> = Vec::new();
    let mut notes: Vec<KeptNote> = Vec::new();
    // One sentence, proposed once.
    //
    // Deduplicated at the CANDIDATE level rather than per list, so a sentence
    // is proposed or noted for what it is only the first time it is read. The
    // same request read twice was set aside above, before any sentence of it
    // was cut; every candidate here is a sentence of a distinct request.
    //
    // A later copy is never dropped and never becomes a second criterion. A
    // sentence the person wrote twice in the SAME request is kept as a note
    // that says it repeats an earlier line (`QUAL-F11-LAUNCH-R5`); a sentence
    // that repeats a line of ANOTHER request — the purpose and a prompt that
    // share a line — is kept as that request's own note, saying so, so no
    // sentence of any request disappears (`QUAL-F11-LAUNCH-R6`).
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for candidate in candidates {
        let key = normalized(&candidate.text);
        match seen.get(&key) {
            Some(first) => {
                let refusal = if *first == candidate.source.ask_id {
                    ShapeRefusal::Repeated
                } else {
                    ShapeRefusal::RepeatsAnotherRequest
                };
                notes.push(KeptNote {
                    text: candidate.text,
                    source: candidate.source,
                    refusal,
                    reason_slug: refusal.slug(),
                });
                continue;
            }
            None => {
                seen.insert(key, candidate.source.ask_id.clone());
            }
        }
        // Too short to be one thing to check: kept, with the reason
        // (`QUAL-F11-LAUNCH-R5`).
        if candidate.too_short {
            notes.push(KeptNote {
                text: candidate.text,
                source: candidate.source,
                refusal: ShapeRefusal::TooShort,
                reason_slug: ShapeRefusal::TooShort.slug(),
            });
            continue;
        }
        // A piece of a sentence never becomes something that counts as done,
        // whatever shape it happens to have (`T-2-8-F1`). It is kept, with the
        // reason, so nothing a person wrote is dropped without saying so.
        if !candidate.whole {
            notes.push(KeptNote {
                text: candidate.text,
                source: candidate.source,
                refusal: ShapeRefusal::Fragment,
                reason_slug: ShapeRefusal::Fragment.slug(),
            });
            continue;
        }
        match observable_shape(&candidate.text) {
            Ok(shape) => {
                let text = criterion_text(&candidate.text);
                criteria.push(ProposedCriterion {
                    number: criteria.len() as u32 + 1,
                    subject_key: shape.subject_key(),
                    text,
                    shape,
                    source: candidate.source,
                    reasoning_slug: "proposal_reasoning",
                });
            }
            Err(refusal) => notes.push(KeptNote {
                text: candidate.text,
                source: candidate.source,
                refusal,
                reason_slug: refusal.slug(),
            }),
        }
    }

    // The paths the profile is read off: what the confirmed sessions changed,
    // plus what the standing scope already covers. Both are stored fields.
    let standing = standing_scope(ledger, change_id)?;
    let mut changed: BTreeSet<String> = BTreeSet::new();
    for session_id in &sessions {
        for path in change_link::session_changed_paths_for_change(ledger, change_id, session_id)? {
            changed.insert(path);
        }
    }
    let mut paths: BTreeSet<String> = changed.clone();
    if let Some((_, scope)) = &standing {
        for entry in &scope.allowed_scope {
            paths.insert(entry.clone());
        }
    }
    let path_list: Vec<String> = paths.into_iter().collect();
    let profile = suggest_profile(&path_list, environment, &mut record);

    // What this Change covers, read off the same stored rows (T-3-2b). The
    // folders are the ones the confirmed work's own files live in; nothing
    // here reads the disk, and nothing widens a folder nobody touched.
    let changed_list: Vec<String> = changed.into_iter().collect();
    let all_entries = scope_for_paths(&changed_list);
    let not_shown = all_entries.len().saturating_sub(MAX_SCOPE_PATHS) as u64;
    let shown: Vec<String> = all_entries.into_iter().take(MAX_SCOPE_PATHS).collect();
    let scope_empty = shown.is_empty();
    let proposed_scope = ProposedScope {
        paths: mask_list(shown, &mut record),
        from_files: changed_list.len() as u64,
        not_shown,
        bound: MAX_SCOPE_PATHS as u64,
        reason_slug: "scope_from_the_work",
        empty_slug: if scope_empty {
            Some("scope_none_found")
        } else {
            None
        },
        over_bound_slug: if not_shown > 0 {
            Some("scope_over_the_bound")
        } else {
            None
        },
    };

    let (accepted, diff) = match &standing {
        Some((version, scope)) => {
            let acceptance = read_acceptance(ledger, change_id, *version, &mut record)?;
            let scope_attribution = scope_attribution_of(&scope.allowed_scope, &proposed_scope);
            let echo = AcceptedEcho {
                version: *version,
                criteria: attribute(&scope.criteria, &criteria, &mut record),
                check_profile: mask_list(scope.check_profile.clone(), &mut record),
                allowed_scope: mask_list(scope.allowed_scope.clone(), &mut record),
                scope_attribution,
                scope_attribution_slug: scope_attribution.slug(),
                acceptance,
                not_an_approval_slug: "accept_is_not_approval",
            };
            let diff = diff_against_accepted(*version, &scope.criteria, &criteria);
            (Some(echo), Some(diff))
        }
        None => (None, None),
    };

    let empty_slug = if criteria.is_empty() {
        Some("proposal_none")
    } else {
        None
    };
    // "Nothing is accepted" covers both a Change with no version at all and a
    // version nobody has agreed to. The second is the one that matters: a scope
    // revision with no acceptance record beside it is a list a surface could
    // easily draw as agreed, and this is the sentence that stops it.
    let not_accepted_slug = if accepted
        .as_ref()
        .is_none_or(|echo| echo.acceptance.is_none())
    {
        Some("accepted_scope_none")
    } else {
        None
    };

    Ok(ProposalView {
        schema_version: PROPOSAL_VIEW_SCHEMA_VERSION,
        change_id: change_id.to_string(),
        request,
        confirmed_sessions: sessions,
        criteria,
        notes,
        profile,
        scope: proposed_scope,
        accepted,
        diff,
        versions_used: version_count(ledger, change_id)?,
        versions_allowed: MAX_SCOPE_VERSIONS,
        versions_slug: "scope_versions_bounded",
        empty_slug,
        not_accepted_slug,
        masking: record.to_treatment(),
    })
}

/// Whose words each accepted line is in.
///
/// Derived rather than stored, and the comparison is stated on the surface: a
/// line is "as proposed" exactly when this store's proposer produces it now.
/// The honest consequence, which the technical register says out loud: if the
/// proposer's own rules change, a line accepted long ago may stop matching and
/// will then be attributed to the person who accepted it. That direction is the
/// safe one — the tool never claims authorship of words it did not produce.
/// Whose list the accepted folders are.
///
/// Every accepted folder is one this store proposes now → the list is as
/// proposed. Anything else is the person's own, and it is attributed to them
/// rather than to us — the same direction `attribute` takes for a criterion,
/// and for the same reason: this product never claims authorship of a line it
/// did not produce.
fn scope_attribution_of(accepted: &[String], proposed: &ProposedScope) -> Attribution {
    if !accepted.is_empty() && accepted.iter().all(|p| proposed.paths.contains(p)) {
        Attribution::AsProposed
    } else {
        Attribution::YourWords
    }
}

fn attribute(
    accepted: &[String],
    proposed: &[ProposedCriterion],
    record: &mut MaskRecord,
) -> Vec<AcceptedCriterion> {
    let proposed_now: BTreeSet<String> = proposed.iter().map(|p| normalized(&p.text)).collect();
    accepted
        .iter()
        .map(|line| {
            let attribution = if proposed_now.contains(&normalized(line)) {
                Attribution::AsProposed
            } else {
                Attribution::YourWords
            };
            AcceptedCriterion {
                text: mask_text(line, record),
                attribution,
                attribution_slug: attribution.slug(),
                not_observable_slug: observable_shape(line)
                    .err()
                    .map(|_| "criterion_not_observable_as_written"),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The acceptance act
// ---------------------------------------------------------------------------

/// Which proposals a person is accepting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Every criterion this reading proposes.
    All,
    /// These numbers, as `propose` numbered them.
    Numbers(Vec<u32>),
}

/// What the person accepts as the folders this Change covers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ScopeChoice {
    /// The folders this store proposed, unchanged. The ordinary path.
    #[default]
    AsProposed,
    /// The person's own list, typed or edited before they accepted it. Their
    /// act, attributed to them.
    Paths(Vec<String>),
}

/// One acceptance, as the caller asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acceptance {
    pub selection: Selection,
    /// What this Change covers (T-3-2b). Accepted in the same act as the
    /// criteria: a Change whose criteria are written down and whose covered
    /// folders are not is a Change under which the scope check refuses every
    /// piece of work, with no surface saying why.
    pub scope: ScopeChoice,
    /// The person says, in as many words, that they are replacing lines they
    /// already accepted with something that says less — or dropping them.
    ///
    /// Default `false`, and [`accept`] refuses rather than doing it quietly.
    /// This flag exists because a hand walkthrough found the hole: accept a
    /// rewritten line, then accept the whole proposed set again, and the second
    /// act silently put the tool's weaker wording back over the person's. The
    /// refusal is `B5.5` applied to the acceptance act itself, which is where it
    /// actually bites.
    pub replacing: bool,
    /// A person's replacement for one proposal, by number. The proposal is not
    /// edited — the accepted line carries the person's words and the proposal
    /// is still derivable from the record.
    pub edits: BTreeMap<u32, String>,
    pub actor: String,
    pub rationale: String,
    pub accepted_at: String,
}

/// What the acceptance wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptanceOutcome {
    pub schema_version: u32,
    pub change_id: String,
    pub version: u32,
    pub criteria: Vec<AcceptedCriterion>,
    pub check_profile: Vec<String>,
    /// What this version says the Change covers, and whose list it is.
    pub allowed_scope: Vec<String>,
    pub scope_attribution: Attribution,
    pub scope_attribution_slug: &'static str,
    pub actor: String,
    pub accepted_at: String,
    pub rationale: String,
    /// The copy-file slug for what just happened.
    pub sentence_slug: &'static str,
    /// `B5.7`, on the same screen as the acceptance itself.
    pub not_an_approval_slug: &'static str,
    pub masking: crate::redact::Treatment,
}

/// One line about to be accepted, in the shape [`diff_against_accepted`] pairs
/// on.
///
/// A line that cannot be cut into three parts gets a pairing key nothing else
/// can match, so two unreadable lines never pair with each other and get
/// compared as though they were about the same thing.
fn synthetic_criterion(number: u32, text: &str) -> ProposedCriterion {
    let shape = observable_shape(text).unwrap_or_else(|_| ObservableShape {
        subject: String::new(),
        condition: String::new(),
        outcome: String::new(),
    });
    let subject_key = match shape.subject.is_empty() {
        true => format!("\u{1f}unreadable-{number}"),
        false => shape.subject_key(),
    };
    ProposedCriterion {
        number,
        text: text.to_string(),
        shape,
        subject_key,
        source: AskSource {
            ask_id: String::new(),
            origin: AskOrigin::ChangePurpose,
            session_id: None,
            event_id: None,
            offset: 0,
            length: 0,
        },
        reasoning_slug: "proposal_reasoning",
    }
}

fn refusal(slug: &'static str, count: u64, version: u64) -> CoreError {
    CoreError::Config(
        crate::voice::proposal_sentence(slug, count, version, "")
            .map(|(plain, _)| plain)
            .unwrap_or_else(|| format!("refused: {slug}")),
    )
}

/// What one acceptance carries besides the proposals chosen by number (C30,
/// HQ "What counts as done": add a line, add to out of scope).
///
/// Both lists are written by the acceptance act and by nothing else: adding a
/// line on a screen is not accepting it, and nothing here is stored until a
/// named person accepts a version that holds it. There is no proposals table
/// (module docs), so a line nobody accepted is not kept anywhere.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Additions {
    /// Lines accepted exactly as written, after the chosen proposals, in this
    /// order: a line the person wrote themselves, or a line of the standing
    /// version they are keeping. Whose words each one is in is derived the way
    /// it is for every accepted line ([`attribute`]), never claimed here.
    pub lines: Vec<String>,
    /// Out of scope: what this Change is not about, added to the exclusions
    /// the standing version already holds. It never removes one, for the same
    /// reason the covered folders are a union: a later version that quietly
    /// lost an exclusion would be a change nobody was shown.
    pub exclusions: Vec<String>,
}

/// Accept a proposed set, or part of it, as what counts as done.
///
/// [`accept_with`] with nothing added: every caller from before C30 reaches
/// exactly the same act through here.
pub fn accept(
    ledger: &Ledger,
    change_id: &str,
    acceptance: &Acceptance,
    environment: &ProfileEnvironment,
) -> Result<AcceptanceOutcome> {
    accept_with(
        ledger,
        change_id,
        acceptance,
        &Additions::default(),
        environment,
    )
}

/// Accept a proposed set, or part of it, plus the person's own lines and
/// out-of-scope items ([`Additions`]), as what counts as done.
///
/// The one write in this module, and the only thing it can write is a scope
/// version and its acceptance record. Five refusals, each with a sentence:
///
/// * no actor — `B5.6` needs one, and an acceptance nobody signed is not one;
/// * nothing selected — an empty list is not a list of what counts as done;
/// * a number nobody proposed — refused rather than ignored;
/// * the bound on versions reached;
/// * nothing new — the selected set is already the standing version, and a
///   version that changes nothing burns one of the bounded slots.
///
/// It does **not** refuse an edit that cannot be cut into the three parts. The
/// person's words are kept exactly, and the view marks them "we cannot tell
/// what would satisfy this" beside the line. Refusing there would be the wall
/// rule 8 forbids; hiding it would be the silent weakening `B5.5` forbids.
///
/// Two more refusals belong to [`Additions`]: an empty line or out-of-scope
/// item, and a line that is already in the list being accepted. Each says so
/// and nothing is written.
pub fn accept_with(
    ledger: &Ledger,
    change_id: &str,
    acceptance: &Acceptance,
    additions: &Additions,
    environment: &ProfileEnvironment,
) -> Result<AcceptanceOutcome> {
    if acceptance.actor.trim().is_empty() {
        return Err(refusal("accept_needs_actor", 0, 0));
    }
    let view = propose(ledger, change_id, environment)?;
    if view.versions_used >= MAX_SCOPE_VERSIONS {
        return Err(refusal(
            "accept_version_limit",
            view.versions_used as u64,
            MAX_SCOPE_VERSIONS as u64,
        ));
    }

    let chosen: Vec<u32> = match &acceptance.selection {
        Selection::All => view.criteria.iter().map(|c| c.number).collect(),
        Selection::Numbers(numbers) => {
            let mut sorted: Vec<u32> = numbers.clone();
            sorted.sort_unstable();
            sorted.dedup();
            sorted
        }
    };
    let mut criteria: Vec<String> = Vec::new();
    for number in &chosen {
        let Some(proposal) = view.criteria.iter().find(|c| c.number == *number) else {
            return Err(refusal("accept_unknown_criterion", *number as u64, 0));
        };
        let text = match acceptance.edits.get(number) {
            Some(edited) if !edited.trim().is_empty() => edited.trim().to_string(),
            _ => proposal.text.clone(),
        };
        criteria.push(text);
    }
    // An edit of a number nobody selected is a request the store would silently
    // lose. Said rather than dropped.
    for number in acceptance.edits.keys() {
        if !chosen.contains(number) {
            return Err(refusal("accept_edit_not_selected", *number as u64, 0));
        }
    }
    // The person's own lines, after the chosen proposals. Refused rather than
    // skipped when empty or already in the list: skipping either would save a
    // different list from the one the person was looking at.
    for line in &additions.lines {
        let text = line.trim();
        if text.is_empty() {
            return Err(CoreError::Config(
                "One of the lines to accept is empty, so nothing was saved. Write the line, or leave it out."
                    .to_string(),
            ));
        }
        if criteria.iter().any(|c| normalized(c) == normalized(text)) {
            return Err(CoreError::Config(format!(
                "“{}” is in this list twice, so nothing was saved. Keep it once.",
                mask_text(text, &mut MaskRecord::default())
            )));
        }
        criteria.push(text.to_string());
    }
    if criteria.is_empty() {
        return Err(refusal("accept_needs_a_criterion", 0, 0));
    }

    // Everything the standing version already said, carried forward. A new
    // version that quietly dropped an exclusion or a boundary would be the
    // weakening `B5.5` forbids, arriving through a field nobody was looking at.
    let standing = standing_scope(ledger, change_id)?;
    let mut scope = match &standing {
        Some((_, previous)) => AcceptedScope {
            criteria: Vec::new(),
            exclusions: previous.exclusions.clone(),
            allowed_scope: previous.allowed_scope.clone(),
            constraints: previous.constraints.clone(),
            check_profile: previous.check_profile.clone(),
        },
        None => AcceptedScope::default(),
    };
    scope.criteria = criteria;
    // Out of scope (C30): added after what the standing version already
    // excludes, each once, never removing one.
    for item in &additions.exclusions {
        let text = item.trim();
        if text.is_empty() {
            return Err(CoreError::Config(
                "One of the things marked out of scope is empty, so nothing was saved. Write it, or leave it out."
                    .to_string(),
            ));
        }
        if !scope.exclusions.iter().any(|e| e.trim() == text) {
            scope.exclusions.push(text.to_string());
        }
    }
    // What this Change covers, written in the same act (T-3-2b). A union with
    // whatever the standing version already covered, never a replacement: a
    // folder an earlier acceptance covered does not stop being covered because
    // a later reading of the confirmed work did not name it again. Narrowing is
    // a re-scope, and it is a different act.
    let chosen_scope: Vec<String> = match &acceptance.scope {
        // AND IT NEVER WIDENS WHAT IS ALREADY COVERED. A Change that already
        // covers folders keeps exactly those until a person says otherwise:
        // accepting a criterion is not agreeing that everything the work has
        // since touched is now in scope, and a scope that grew on its own
        // would let work nobody agreed to start counting.
        ScopeChoice::AsProposed if scope.allowed_scope.is_empty() => view.scope.paths.clone(),
        ScopeChoice::AsProposed => Vec::new(),
        ScopeChoice::Paths(paths) => paths
            .iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
    };
    let mut allowed: BTreeSet<String> = scope.allowed_scope.iter().cloned().collect();
    for path in &chosen_scope {
        allowed.insert(path.clone());
    }
    scope.allowed_scope = allowed.into_iter().collect();
    // The profile is a union, never a replacement: a check the standing version
    // asked for does not stop being needed because a later reading of the paths
    // did not ask for it again.
    let mut profile: BTreeSet<String> = scope.check_profile.iter().cloned().collect();
    for check in &view.profile.checks {
        profile.insert(check.id.to_string());
    }
    scope.check_profile = profile.into_iter().collect();

    if let Some((version, previous)) = &standing {
        if previous.content_sha256() == scope.content_sha256() {
            return Err(refusal("accept_nothing_new", 0, *version as u64));
        }
        // Never silently weaken, inside the act.
        //
        // The same two mechanical tests [`diff_against_accepted`] applies to a
        // re-proposal, applied to what is about to be written: if this list
        // drops a line the standing version holds, or says less than one, the
        // act is refused and names how many. A person may of course change their
        // mind — they say so, and then it goes through.
        if !previous.criteria.is_empty() && !acceptance.replacing {
            // First: a line the person wrote themselves may not be replaced by
            // our wording without them saying so.
            //
            // The two weakening tests do not catch this, and the hand
            // walkthrough showed why: "the record shows exactly one failure
            // row" replaced by "the record shows a single failure row" is
            // neither strictly weaker nor a dropped negation — it is our words
            // over theirs, which is the harm `B5.5` is actually about.
            let kept: BTreeSet<String> = scope.criteria.iter().map(|c| normalized(c)).collect();
            let overwritten = attribute(
                &previous.criteria,
                &view.criteria,
                &mut MaskRecord::default(),
            )
            .into_iter()
            .filter(|line| line.attribution == Attribution::YourWords)
            .filter(|line| !kept.contains(&normalized(&line.text)))
            .count();
            if overwritten > 0 {
                return Err(refusal(
                    "accept_would_overwrite_your_words",
                    overwritten as u64,
                    *version as u64,
                ));
            }
            let proposed_now: Vec<ProposedCriterion> = scope
                .criteria
                .iter()
                .enumerate()
                .map(|(index, text)| synthetic_criterion(index as u32 + 1, text))
                .collect();
            let diff = diff_against_accepted(*version, &previous.criteria, &proposed_now);
            let harmed = diff.weakened.len() + diff.removed.len();
            if harmed > 0 {
                return Err(refusal(
                    "accept_would_weaken",
                    harmed as u64,
                    *version as u64,
                ));
            }
        }
    }

    // One transaction: a scope version with no acceptance record beside it is a
    // list nobody agreed to that a later reader would show as standing.
    let transaction = ledger.conn().unchecked_transaction()?;
    let version = change::append_scope(ledger, change_id, &acceptance.accepted_at, &scope)?;
    change::accept_scope(
        ledger,
        change_id,
        version,
        &ScopeAcceptance {
            actor: acceptance.actor.clone(),
            accepted_at: acceptance.accepted_at.clone(),
            rationale: acceptance.rationale.clone(),
        },
    )?;
    transaction.commit()?;

    let mut record = MaskRecord::default();
    let scope_attribution = scope_attribution_of(&scope.allowed_scope, &view.scope);
    Ok(AcceptanceOutcome {
        schema_version: PROPOSAL_VIEW_SCHEMA_VERSION,
        change_id: change_id.to_string(),
        version,
        criteria: attribute(&scope.criteria, &view.criteria, &mut record),
        check_profile: mask_list(scope.check_profile.clone(), &mut record),
        allowed_scope: mask_list(scope.allowed_scope.clone(), &mut record),
        scope_attribution,
        scope_attribution_slug: scope_attribution.slug(),
        actor: mask_text(&acceptance.actor, &mut record),
        accepted_at: acceptance.accepted_at.clone(),
        rationale: mask_text(&acceptance.rationale, &mut record),
        sentence_slug: "accept_written",
        not_an_approval_slug: "accept_is_not_approval",
        masking: record.to_treatment(),
    })
}
