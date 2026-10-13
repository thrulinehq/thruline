//! QUAL-F11-LAUNCH-R5 — no sentence of the ask disappears: a very short
//! sentence and a repeated sentence are kept as notes with a plain reason,
//! never dropped silently.
//!
//! Round 4's adversary (`gates/adversary-QUAL-F11-LAUNCH-R4`) found two ways
//! the proposer lost words of the ask with no quotation mark involved:
//!
//! 1. a sentence shorter than the proposer's minimum ("Retry now.", "; Do
//!    it.") was thrown away before it became either a criterion or a note;
//! 2. a sentence the person wrote twice in the same request was skipped the
//!    second time, with nothing said.
//!
//! # The rule these tests hold
//!
//! Every sentence of a request becomes exactly one of: a proposed criterion,
//! or a kept note with a reason. A sentence too short to check on its own is
//! a note (`note_too_short`). A sentence that repeats an earlier line of the
//! same request is a note that says so (`note_repeats_an_earlier_line`); the
//! earlier line keeps whatever it was, so a repeat never becomes a second
//! criterion. The words of the criteria plus the words of the notes, in the
//! request's order, are the words of the request: nothing dropped, nothing
//! duplicated, nothing invented.
//!
//! # The adversary's corpus, rebuilt
//!
//! `the_adversarys_r4_corpus_rebuilt_keeps_every_word` rebuilds the round-4
//! adversary's own 2,000-ask corpus from its building blocks — seed
//! [`ADV_SEED`] (`0xadfe5f1120260929`), the same linear congruential
//! generator, the same blocks ([`NESTED`], [`GUILLEMET`], [`CJK`],
//! [`NAME_POSSESSIVE`], [`ELISION_BLOCK`], [`ABBREV_IN_QUOTE`],
//! [`EMPTY_QUOTE`], [`LONE_APOSTROPHE`], [`PLAIN_QUOTE`]), the same four
//! sentence templates and two or three sentences per ask — and checks every
//! ask against the adversary's oracle: (a) word preservation, (b) no
//! criterion ends inside a quotation under any reading, (c) no cut right
//! after a mark with two readings. At d19ef51f, 31 of its asks failed (a).
//!
//! # Across requests (`QUAL-F11-LAUNCH-R6`)
//!
//! A Change is read out of more than one request: its purpose, the prompt it
//! points at, and each confirmed session's opening. Round 5's adversary found
//! a sentence repeating one from ANOTHER request skipped with nothing said.
//! Section 5 holds the same rule per request: a later copy of a line from
//! another request is a note (`note_repeats_a_line_of_another_request`), only
//! the first occurrence can be a criterion, and
//! `the_multi_request_corpus_keeps_every_word_of_every_request` checks every
//! request of 600 generated Changes (seed [`MULTI_SEED`]).
//!
//! Every id is synthetic (`cccccccc-…`) and no fixture here holds private
//! content.

use std::collections::BTreeSet;

use thruline_core::change::{self, AssociationProvenance, ChangeRequest, NewChange, RequestSource};
use thruline_core::change_link;
use thruline_core::change_propose::{self, AskOrigin, ProfileEnvironment, ProposalView};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000f0a5";

/// The note a sentence too short to check on its own carries.
const TOO_SHORT: &str = "note_too_short";
/// The note a sentence repeating an earlier line of the same request carries.
const REPEATS: &str = "note_repeats_an_earlier_line";

struct Store {
    _dir: tempfile::TempDir,
    ledger: Ledger,
    next: u64,
}

impl Store {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id, root_path, source, display_name, created_at)
                 VALUES(?1, '/tmp/synthetic-f11-r5', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
                rusqlite::params![PROJECT],
            )
            .expect("insert project");
        Store {
            _dir: dir,
            ledger,
            next: 0,
        }
    }

    fn propose(&mut self, purpose: &str) -> ProposalView {
        self.next += 1;
        let id = format!("cccccccc-0000-4000-8000-{:012x}", self.next);
        change::insert_change(
            &self.ledger,
            &NewChange {
                id: id.clone(),
                project_id: PROJECT.to_string(),
                opened_at: "2026-01-02T09:00:00Z".to_string(),
                request: ChangeRequest {
                    title: "Synthetic".to_string(),
                    purpose: purpose.to_string(),
                    prompt_event_id: None,
                    requester: "sam".to_string(),
                    source: RequestSource::Person,
                },
            },
        )
        .expect("insert the change");
        change_propose::propose(&self.ledger, &id, &ProfileEnvironment::empty()).expect("propose")
    }

    /// A Change read out of more than one request (`QUAL-F11-LAUNCH-R6`): its
    /// purpose, the original prompt it points at, and the opening request of
    /// one session a person confirmed belongs to it. Each is its own row, in
    /// its own place in the record. `Opening::SameAsPrompt` is the commonest
    /// real configuration: the session opens with the very event the Change
    /// points at, so the same request is read twice.
    fn propose_requests(
        &mut self,
        purpose: &str,
        prompt: Option<&str>,
        opening: Opening<'_>,
    ) -> ProposalView {
        self.next += 1;
        let n = self.next;
        let id = format!("cccccccc-0000-4000-8000-{n:012x}");
        let session = format!("cccccccc-0000-4000-8002-{n:012x}");
        let prompt_event = format!("cccccccc-0000-4000-8001-{n:012x}");
        let opening_event = format!("cccccccc-0000-4000-8003-{n:012x}");
        let has_session = !matches!(opening, Opening::None);
        if has_session {
            self.ledger
                .conn()
                .execute(
                    "INSERT INTO sessions(id, project_id, file_path, started_at)
                     VALUES(?1, ?2, '/tmp/synthetic-f11-r6/s.jsonl', '2026-01-02T08:00:00Z')",
                    rusqlite::params![session, PROJECT],
                )
                .expect("insert session");
        }
        let add_event = |event: &str, session: &str, seq: i64, text: &str| {
            self.ledger
                .conn()
                .execute(
                    "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                         raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                     VALUES(?1, ?2, ?3, ?4, '2026-01-02T08:30:00Z', 'user_msg', ?5,
                         '/tmp/synthetic-f11-r6/s.jsonl', ?4, 'x', '', '')",
                    rusqlite::params![event, PROJECT, session, seq, text],
                )
                .expect("insert event");
        };
        let mut prompt_event_id = None;
        match (prompt, opening) {
            (Some(text), Opening::SameAsPrompt) => {
                add_event(&prompt_event, &session, 1, text);
                prompt_event_id = Some(prompt_event.clone());
            }
            (prompt, opening) => {
                if let Some(text) = prompt {
                    let elsewhere = format!("cccccccc-0000-4000-8004-{n:012x}");
                    add_event(&prompt_event, &elsewhere, 1, text);
                    prompt_event_id = Some(prompt_event.clone());
                }
                if let Opening::Own(text) = opening {
                    add_event(&opening_event, &session, 1, text);
                }
            }
        }
        change::insert_change(
            &self.ledger,
            &NewChange {
                id: id.clone(),
                project_id: PROJECT.to_string(),
                opened_at: "2026-01-02T09:00:00Z".to_string(),
                request: ChangeRequest {
                    title: "Synthetic".to_string(),
                    purpose: purpose.to_string(),
                    prompt_event_id,
                    requester: "sam".to_string(),
                    source: RequestSource::Person,
                },
            },
        )
        .expect("insert the change");
        if has_session {
            change_link::decide(
                &self.ledger,
                &id,
                &session,
                AssociationProvenance::Confirmed,
                "sam",
                "this is the work",
                "2026-01-02T09:10:00Z",
            )
            .expect("confirm the session");
        }
        change_propose::propose(&self.ledger, &id, &ProfileEnvironment::empty()).expect("propose")
    }
}

/// The opening request of the one confirmed session, if there is one.
#[derive(Clone, Copy)]
enum Opening<'a> {
    None,
    Own(&'a str),
    SameAsPrompt,
}

fn criterion_texts(view: &ProposalView) -> Vec<String> {
    view.criteria.iter().map(|c| c.text.clone()).collect()
}

fn note_texts(view: &ProposalView) -> Vec<String> {
    view.notes.iter().map(|n| n.text.clone()).collect()
}

fn notes_with(view: &ProposalView, slug: &str) -> Vec<String> {
    view.notes
        .iter()
        .filter(|n| n.reason_slug == slug)
        .map(|n| n.text.clone())
        .collect()
}

fn words_of(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.trim_end_matches(['.', ';', '!', '?', ',']).to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// (a) word preservation: the words of criteria + notes, in the order of the
/// offsets they were read from, equal the words of the ask.
fn check_word_preservation(ask: &str, view: &ProposalView) -> Result<(), String> {
    words_hold(ask, &spans_of(view, None))
}

/// One kept span: where it was read from, how long, its text, and whether it
/// is a criterion.
type Span = (usize, usize, String, bool);

/// The spans of every criterion and note, or only of those read out of one
/// request when `origin` names it (`QUAL-F11-LAUNCH-R6`).
fn spans_of(view: &ProposalView, origin: Option<AskOrigin>) -> Vec<Span> {
    let mut spans: Vec<Span> = view
        .criteria
        .iter()
        .filter(|c| origin.is_none_or(|o| c.source.origin == o))
        .map(|c| (c.source.offset, c.source.length, c.text.clone(), true))
        .chain(
            view.notes
                .iter()
                .filter(|n| origin.is_none_or(|o| n.source.origin == o))
                .map(|n| (n.source.offset, n.source.length, n.text.clone(), false)),
        )
        .collect();
    spans.sort();
    spans
}

fn words_hold(ask: &str, spans: &[Span]) -> Result<(), String> {
    let kept: Vec<String> = spans.iter().flat_map(|(_, _, t, _)| words_of(t)).collect();
    let expected = words_of(ask);
    if kept != expected {
        return Err(format!(
            "(a) words differ:\n  ask:  {expected:?}\n  kept: {kept:?}"
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 1. The adversary's minimal cases (red at d19ef51f)
// ---------------------------------------------------------------------------

/// "Retry now." is nine characters. It is kept as a note that says it is too
/// short to check on its own, and it is not a criterion.
#[test]
fn a_short_trailing_sentence_is_kept_as_a_note() {
    let mut store = Store::new();
    let ask = "The panel shows the report when the job ends. Retry now.";
    let view = store.propose(ask);
    assert_eq!(
        criterion_texts(&view),
        vec!["The panel shows the report when the job ends.".to_string()]
    );
    assert_eq!(notes_with(&view, TOO_SHORT), vec!["Retry now".to_string()]);
    assert_eq!(view.notes.len(), 1, "{:?}", note_texts(&view));
    check_word_preservation(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
}

/// The same after a semicolon: "Do it." is its own sentence and is kept.
#[test]
fn a_short_sentence_after_a_semicolon_is_kept_as_a_note() {
    let mut store = Store::new();
    let ask = "The panel shows the report when the job runs; Do it.";
    let view = store.propose(ask);
    assert_eq!(
        criterion_texts(&view),
        vec!["The panel shows the report when the job runs.".to_string()]
    );
    assert_eq!(notes_with(&view, TOO_SHORT), vec!["Do it".to_string()]);
    check_word_preservation(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
}

/// A short sentence first, in the middle and last: every one is kept, in
/// place.
#[test]
fn a_short_sentence_anywhere_is_kept_in_place() {
    let mut store = Store::new();
    for ask in [
        "Hurry! When the cart is empty, the total shows zero.",
        "When the cart is empty, the total shows zero. Why? The total shows the sum of the prices.",
        "When the cart is empty, the total shows zero. Ship it.",
        "When the cart is empty, the total shows zero.\n\nThanks.",
    ] {
        let view = store.propose(ask);
        assert_eq!(
            notes_with(&view, TOO_SHORT).len(),
            1,
            "{ask:?}: {:?}",
            note_texts(&view)
        );
        assert!(
            criterion_texts(&view).iter().all(|c| c.len() >= 12),
            "{ask:?}: a short sentence was proposed: {:?}",
            criterion_texts(&view)
        );
        check_word_preservation(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
    }
}

/// A sentence written twice in one request is proposed once; the second time
/// is a note that says it repeats an earlier line.
#[test]
fn a_repeated_sentence_is_kept_as_a_note_the_second_time() {
    let mut store = Store::new();
    let ask = "The service shows delivered when the job ends. The service shows delivered when the job ends.";
    let view = store.propose(ask);
    assert_eq!(
        criterion_texts(&view),
        vec!["The service shows delivered when the job ends.".to_string()],
        "a repeat never becomes a second criterion"
    );
    assert_eq!(
        notes_with(&view, REPEATS),
        vec!["The service shows delivered when the job ends".to_string()]
    );
    assert_eq!(view.notes.len(), 1, "{:?}", note_texts(&view));
    // The note points at the second sentence, not the first.
    let note = &view.notes[0];
    assert_eq!(
        note.source.offset,
        ask.rfind("The service").expect("second")
    );
    check_word_preservation(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
}

/// A repeat that differs only in case and punctuation is still a repeat; a
/// repeated note (a sentence that was never a criterion) is still kept; and
/// a third time is a third line.
#[test]
fn every_repeat_is_kept_whatever_the_first_one_became() {
    let mut store = Store::new();
    let ask = "Add a cart total function. When the cart is empty, the total shows zero. add a cart total function! When the cart is empty, the total shows zero; When the cart is empty, the total shows zero.";
    let view = store.propose(ask);
    assert_eq!(
        criterion_texts(&view),
        vec!["When the cart is empty, the total shows zero.".to_string()]
    );
    assert_eq!(
        notes_with(&view, REPEATS).len(),
        3,
        "{:?}",
        note_texts(&view)
    );
    check_word_preservation(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
}

/// A short sentence written twice: both are kept, neither is a criterion.
#[test]
fn a_short_sentence_written_twice_is_kept_twice() {
    let mut store = Store::new();
    let ask = "When the cart is empty, the total shows zero. Do it. Do it.";
    let view = store.propose(ask);
    assert_eq!(view.criteria.len(), 1);
    assert_eq!(view.notes.len(), 2, "{:?}", note_texts(&view));
    check_word_preservation(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
}

/// The two new notes each have a sentence in the copy file, in both
/// registers, so a person is never told "no" without the reason.
#[test]
fn both_new_notes_have_a_sentence_in_the_copy_file() {
    for slug in [TOO_SHORT, REPEATS] {
        let line = thruline_core::voice::PROPOSAL_LINES
            .iter()
            .find(|l| l.slug == slug)
            .unwrap_or_else(|| panic!("{slug} has no line in the copy file"));
        assert!(line
            .plain
            .starts_with("Kept as a note, not something to check:"));
    }
}

// ---------------------------------------------------------------------------
// 2. The adversary's hand attacks that lost a short tail (red at d19ef51f)
// ---------------------------------------------------------------------------

#[test]
fn an_elision_beside_a_proper_name_possessive_keeps_every_word() {
    let mut store = Store::new();
    for ask in [
        "The badge shows the ’90s price 'til Chris' report runs; Retry later.",
        "The badge shows the '90s price 'til Jones' report runs; Retry later.",
    ] {
        let view = store.propose(ask);
        check_invariants(ask, &view);
    }
}

#[test]
fn a_lone_apostrophe_as_a_whole_word_keeps_every_word() {
    let mut store = Store::new();
    for ask in [
        "The panel shows ' delivered when the job ends; Retry later.",
        "The panel shows 'delivered ' now' when the job ends; Retry later.",
    ] {
        let view = store.propose(ask);
        check_invariants(ask, &view);
    }
}

// ---------------------------------------------------------------------------
// 3. Rounds 1–4, pinned again
// ---------------------------------------------------------------------------

const Q02_PURPOSE: &str = "Two retry requests for the same delivery do not enqueue duplicate work. A delivery that was still waiting when the service was stopped is still exactly one delivery after it starts again, and it is attempted again. A delivery that has exhausted its attempts is shown as failed; the service saying 'sent' does not make it sent.";
const WALK_ASK: &str = "Add a cart total function. When the cart is empty, the total shows zero. The total shows the sum of the prices.";

#[test]
fn q02_and_the_walk_still_give_what_they_pinned() {
    let mut store = Store::new();
    let view = store.propose(Q02_PURPOSE);
    assert_eq!(
        criterion_texts(&view),
        vec![
            "Two retry requests for the same delivery do not enqueue duplicate work.".to_string(),
            "A delivery that was still waiting when the service was stopped is still exactly one delivery after it starts again, and it is attempted again.".to_string(),
            "A delivery that has exhausted its attempts is shown as failed; the service saying 'sent' does not make it sent.".to_string(),
        ]
    );
    assert!(view.notes.is_empty());
    let view = store.propose(WALK_ASK);
    assert_eq!(
        criterion_texts(&view),
        vec![
            "When the cart is empty, the total shows zero.".to_string(),
            "The total shows the sum of the prices.".to_string(),
        ]
    );
    assert_eq!(
        note_texts(&view),
        vec!["Add a cart total function".to_string()]
    );
}

// ---------------------------------------------------------------------------
// 4. The adversary's corpus, rebuilt from its building blocks
// ---------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize
    }
    fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        from[self.next() % from.len()]
    }
}

/// The round-4 adversary's seed.
const ADV_SEED: u64 = 0xADFE_5F11_2026_0929;
const ADV_CORPUS_SIZE: usize = 2000;

const SUBJ: [&str; 4] = ["The panel", "The badge", "The banner", "The counter"];
const VERB_TAIL: [&str; 3] = ["shows", "reports", "logs"];
const COND: [&str; 4] = [
    "when the job ends",
    "when the cart is empty",
    "once the retry completes",
    "while the form is open",
];
const NESTED: [&str; 3] = [
    "'the report says 'delivered' now'",
    "‘the report says ‘delivered’ now’",
    "'outer 'inner text' tail'",
];
const GUILLEMET: [&str; 2] = ["«delivered; Retry disabled»", "«Saved. Retry later»"];
const CJK: [&str; 2] = ["「delivered; Retry disabled」", "「Saved。続行」"];
const NAME_POSSESSIVE: [&str; 4] = ["Chris'", "Jones'", "the boss'", "the drivers'"];
const ELISION_BLOCK: [&str; 4] = ["'til", "’til", "'90s", "’em"];
const ABBREV_IN_QUOTE: [&str; 2] = ["'v1.2 shipped; e.g. today'", "\"v2.0 done; i.e. now\""];
const EMPTY_QUOTE: [&str; 2] = ["''", "\"\""];
const LONE_APOSTROPHE: [&str; 1] = ["'"];
const PLAIN_QUOTE: [&str; 4] = [
    "'delivered; Retry disabled'",
    "\"delivered; Retry disabled\"",
    "‘delivered; Retry disabled’",
    "“delivered; Retry disabled”",
];

fn block(rng: &mut Rng) -> String {
    match rng.next() % 9 {
        0 => rng.pick(&NESTED).to_string(),
        1 => rng.pick(&GUILLEMET).to_string(),
        2 => rng.pick(&CJK).to_string(),
        3 => format!(
            "{} report {}",
            rng.pick(&NAME_POSSESSIVE),
            rng.pick(&PLAIN_QUOTE)
        ),
        4 => format!("{} the count", rng.pick(&ELISION_BLOCK)),
        5 => rng.pick(&ABBREV_IN_QUOTE).to_string(),
        6 => rng.pick(&EMPTY_QUOTE).to_string(),
        7 => rng.pick(&LONE_APOSTROPHE).to_string(),
        _ => rng.pick(&PLAIN_QUOTE).to_string(),
    }
}

fn adv_sentence(rng: &mut Rng) -> String {
    let subj = rng.pick(&SUBJ);
    let verb = rng.pick(&VERB_TAIL);
    let cond = rng.pick(&COND);
    let b = block(rng);
    match rng.next() % 4 {
        0 => format!("{subj} {verb} {b} {cond}."),
        1 => format!("{subj} {verb} {b}; {cond}."),
        2 => format!("When {cond}, {subj} {verb} {b}."),
        _ => format!("{subj} {verb} {b}."),
    }
}

fn adv_corpus() -> Vec<String> {
    let mut rng = Rng(ADV_SEED);
    (0..ADV_CORPUS_SIZE)
        .map(|_| {
            let n = 2 + rng.next() % 2;
            (0..n)
                .map(|_| adv_sentence(&mut rng))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

// The adversary's oracle for (b) and (c), re-created: a brute-force reading of
// the single marks of one stretch, independent of `change_propose.rs`.

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Role {
    Open,
    Close,
    Apo,
}

struct Mark {
    at: usize,
    opens: bool,
    closes: bool,
    possessive: bool,
    elision: bool,
}

const ELISIONS: [&str; 10] = [
    "til", "tis", "twas", "em", "cause", "bout", "round", "n", "nuff", "cos",
];

fn find_single_marks(text: &str) -> Vec<Mark> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    for (i, &(at, ch)) in chars.iter().enumerate() {
        if !matches!(ch, '\'' | '‘' | '’') {
            continue;
        }
        let prev = i.checked_sub(1).map(|j| chars[j].1);
        let next = chars.get(i + 1).map(|(_, c)| *c);
        if prev.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric) {
            continue;
        }
        let opens = prev
            .is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '"' | '“' | '”'))
            && next.is_some_and(|c| !c.is_whitespace());
        let closes =
            prev.is_some_and(|c| !c.is_whitespace()) && next.is_none_or(|c| !c.is_alphanumeric());
        let possessive = closes && matches!(prev, Some('s' | 'S'));
        let word: String = chars[i + 1..]
            .iter()
            .map(|(_, c)| *c)
            .take_while(|c| c.is_alphanumeric())
            .collect();
        let elision = opens
            && (word.chars().next().is_some_and(|c| c.is_ascii_digit())
                || ELISIONS.contains(&word.to_lowercase().as_str()));
        out.push(Mark {
            at,
            opens,
            closes,
            possessive,
            elision,
        });
    }
    out
}

fn all_full_readings(marks: &[Mark]) -> Vec<Vec<Role>> {
    fn go(marks: &[Mark], i: usize, open: bool, acc: &mut Vec<Role>, out: &mut Vec<Vec<Role>>) {
        if i == marks.len() {
            if !open {
                out.push(acc.clone());
            }
            return;
        }
        let m = &marks[i];
        let mut options: Vec<(Role, bool)> = Vec::new();
        if open {
            if m.closes {
                options.push((Role::Close, false));
                if m.possessive {
                    options.push((Role::Apo, true));
                }
            } else {
                options.push((Role::Apo, true));
            }
        } else if m.opens {
            options.push((Role::Open, true));
            if m.elision {
                options.push((Role::Apo, false));
            }
        } else if m.possessive {
            options.push((Role::Apo, false));
        }
        for (role, next_open) in options {
            acc.push(role);
            go(marks, i + 1, next_open, acc, out);
            acc.pop();
        }
    }
    let mut out = Vec::new();
    go(marks, 0, false, &mut Vec::new(), &mut out);
    out
}

fn double_open_at(text: &str, at: usize) -> bool {
    let mut open = false;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (i, &(idx, ch)) in chars.iter().enumerate() {
        if idx >= at {
            break;
        }
        let prev = i.checked_sub(1).map(|j| chars[j].1);
        let next = chars.get(i + 1).map(|(_, c)| *c);
        let opens = prev
            .is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '"' | '“' | '”'))
            && next.is_some_and(|c| !c.is_whitespace());
        let closes =
            prev.is_some_and(|c| !c.is_whitespace()) && next.is_none_or(|c| !c.is_alphanumeric());
        match ch {
            '"' => open = !open,
            '“' | '”' if open && closes => open = false,
            '“' | '”' if !open && opens => open = true,
            _ => {}
        }
    }
    open
}

/// (b) and (c) at every cut between two consecutive kept spans. The corpus
/// holds no line break, so the whole ask is one stretch.
fn check_cut_invariants(ask: &str, view: &ProposalView) -> Result<(), String> {
    cuts_hold(ask, &spans_of(view, None))
}

fn cuts_hold(ask: &str, spans: &[Span]) -> Result<(), String> {
    let marks = find_single_marks(ask);
    let readings = all_full_readings(&marks);
    for pair in spans.windows(2) {
        let (from, len, _, is_criterion) = pair[0];
        let (next_from, ..) = pair[1];
        let gap = &ask[from + len..next_from];
        let Some((off, _)) = gap
            .char_indices()
            .find(|(_, c)| matches!(c, '.' | ';' | '!' | '?'))
        else {
            continue;
        };
        let cut = from + len + off;
        if readings.is_empty() {
            continue;
        }
        let before: Vec<usize> = (0..marks.len()).filter(|i| marks[*i].at < cut).collect();
        let ends_its_stretch = ask[cut + 1..].trim().is_empty();
        if !ends_its_stretch {
            for i in &before {
                let roles: BTreeSet<Role> = readings.iter().map(|r| r[*i]).collect();
                if roles.len() > 1 {
                    return Err(format!(
                        "(c) a cut at byte {cut} follows the mark at byte {} with readings {roles:?}",
                        marks[*i].at
                    ));
                }
            }
        }
        if is_criterion {
            for reading in &readings {
                let mut open = false;
                for i in &before {
                    match reading[*i] {
                        Role::Open => open = true,
                        Role::Close => open = false,
                        Role::Apo => {}
                    }
                }
                if open {
                    return Err(format!(
                        "(b) a criterion ends at byte {cut} inside a single quotation"
                    ));
                }
            }
            if double_open_at(ask, cut) {
                return Err(format!(
                    "(b) a criterion ends at byte {cut} inside a double quotation"
                ));
            }
        }
    }
    Ok(())
}

fn check_invariants(ask: &str, view: &ProposalView) {
    check_word_preservation(ask, view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
    check_cut_invariants(ask, view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
}

#[test]
fn the_adversarys_r4_corpus_rebuilt_keeps_every_word() {
    let asks = adv_corpus();
    assert_eq!(asks.len(), ADV_CORPUS_SIZE);
    let mut store = Store::new();
    let mut failures: Vec<String> = Vec::new();
    let (mut criteria, mut notes, mut short, mut repeats) = (0usize, 0usize, 0usize, 0usize);
    for (index, ask) in asks.iter().enumerate() {
        let view = store.propose(ask);
        criteria += view.criteria.len();
        notes += view.notes.len();
        short += notes_with(&view, TOO_SHORT).len();
        repeats += notes_with(&view, REPEATS).len();
        // A repeat never becomes a second criterion.
        let distinct: BTreeSet<String> = criterion_texts(&view)
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        if distinct.len() != view.criteria.len() {
            failures.push(format!("#{index} {ask:?}: a criterion is proposed twice"));
            continue;
        }
        if let Err(e) =
            check_word_preservation(ask, &view).and_then(|()| check_cut_invariants(ask, &view))
        {
            failures.push(format!(
                "#{index} {ask:?}: {e}\n    criteria {:?}\n    notes {:?}",
                criterion_texts(&view),
                note_texts(&view)
            ));
        }
    }
    eprintln!(
        "QUAL-F11-LAUNCH-R5 adversary corpus rebuilt: seed {ADV_SEED:#x}, {} asks, {criteria} criteria, {notes} notes ({short} too short, {repeats} repeats), {} failing",
        asks.len(),
        failures.len()
    );
    assert!(
        repeats > 0,
        "the rebuilt corpus exercises repeated sentences"
    );
    assert!(
        failures.is_empty(),
        "{} of {} asks lose or duplicate words; the first ten:\n{}",
        failures.len(),
        asks.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
}

// ---------------------------------------------------------------------------
// 5. A sentence that repeats one from ANOTHER request (QUAL-F11-LAUNCH-R6;
//    red at 654f1a38)
// ---------------------------------------------------------------------------
//
// Round 5's adversary (`gates/adversary-QUAL-F11-LAUNCH-R5`) found that a
// sentence repeating one read from a different request — the Change's purpose
// and a recorded prompt that happen to share a line — was skipped with
// nothing said: not a criterion, not a note. The rule is the one above, per
// request: every sentence of every request becomes exactly one of a proposed
// criterion or a kept note with a reason. Only the first occurrence can be a
// criterion; a later one, from another request, is a note that says it
// repeats a line of another request. The one thing that is still read once
// is one request read twice — the same recorded event reached both as the
// Change's prompt and as a confirmed session's opening.

/// The note a sentence repeating a line of another request carries.
const REPEATS_ANOTHER: &str = "note_repeats_a_line_of_another_request";

fn slug_of(view: &ProposalView, origin: AskOrigin, text: &str) -> Vec<&'static str> {
    view.notes
        .iter()
        .filter(|n| n.source.origin == origin && n.text == text)
        .map(|n| n.reason_slug)
        .collect()
}

/// The adversary's case: purpose "Ship the release notes." and a prompt
/// "Fix the checkout timer. Ship the release notes." The prompt's second
/// sentence is kept, as the prompt's own note, saying it repeats a line of
/// another request.
#[test]
fn a_sentence_of_the_prompt_that_repeats_the_purpose_is_kept_as_a_note() {
    let mut store = Store::new();
    let purpose = "Ship the release notes.";
    let prompt = "Fix the checkout timer. Ship the release notes.";
    let view = store.propose_requests(purpose, Some(prompt), Opening::None);
    assert!(
        criterion_texts(&view).is_empty(),
        "{:?}",
        criterion_texts(&view)
    );
    assert_eq!(
        note_texts(&view),
        vec![
            "Ship the release notes".to_string(),
            "Fix the checkout timer".to_string(),
            "Ship the release notes".to_string(),
        ],
        "every sentence of both requests, once each"
    );
    // The purpose's own line keeps whatever it was; it is not a repeat.
    let first = slug_of(&view, AskOrigin::ChangePurpose, "Ship the release notes");
    assert_eq!(first.len(), 1);
    assert_ne!(first[0], REPEATS_ANOTHER);
    assert_ne!(first[0], REPEATS);
    // The prompt's copy is the prompt's own note, at its own place.
    assert_eq!(
        slug_of(&view, AskOrigin::OriginalPrompt, "Ship the release notes"),
        vec![REPEATS_ANOTHER]
    );
    let repeat = view
        .notes
        .iter()
        .find(|n| n.reason_slug == REPEATS_ANOTHER)
        .expect("the repeat");
    assert_eq!(repeat.source.offset, prompt.find("Ship").expect("ship"));
    for (origin, ask) in [
        (AskOrigin::ChangePurpose, purpose),
        (AskOrigin::OriginalPrompt, prompt),
    ] {
        words_hold(ask, &spans_of(&view, Some(origin)))
            .unwrap_or_else(|e| panic!("{origin:?} {ask:?}: {e}"));
    }
}

/// Three requests share one checkable sentence. The first occurrence is the
/// one criterion; the prompt's and the session's copies are each a note that
/// says it repeats a line of another request; every other sentence of every
/// request is kept.
#[test]
fn three_requests_sharing_a_sentence_propose_it_once_and_keep_the_rest() {
    let mut store = Store::new();
    let shared = "When the cart is empty, the total shows zero.";
    let purpose = format!("Add a cart total function. {shared}");
    let prompt = format!("{shared} The total shows the sum of the prices.");
    let opening = format!("Ship it today please. {shared}");
    let view = store.propose_requests(&purpose, Some(&prompt), Opening::Own(&opening));
    assert_eq!(
        criterion_texts(&view),
        vec![
            shared.to_string(),
            "The total shows the sum of the prices.".to_string(),
        ],
        "the shared sentence is proposed once, from the first request"
    );
    assert_eq!(
        view.criteria[0].source.origin,
        AskOrigin::ChangePurpose,
        "only the first occurrence can be a criterion"
    );
    let bare = shared.trim_end_matches('.');
    assert_eq!(
        slug_of(&view, AskOrigin::OriginalPrompt, bare),
        vec![REPEATS_ANOTHER]
    );
    assert_eq!(
        slug_of(&view, AskOrigin::SessionOpening, bare),
        vec![REPEATS_ANOTHER]
    );
    for (origin, ask) in [
        (AskOrigin::ChangePurpose, purpose.as_str()),
        (AskOrigin::OriginalPrompt, prompt.as_str()),
        (AskOrigin::SessionOpening, opening.as_str()),
    ] {
        words_hold(ask, &spans_of(&view, Some(origin)))
            .unwrap_or_else(|e| panic!("{origin:?} {ask:?}: {e}"));
    }
}

/// A sentence the purpose and a session share, with a prompt that shares
/// nothing: the session's copy is kept; the prompt is untouched.
#[test]
fn a_session_opening_that_repeats_the_purpose_is_kept_as_a_note() {
    let mut store = Store::new();
    let purpose = "The badge shows the count when the queue drains.";
    let prompt = "The banner logs the error when the upload fails.";
    let opening = "Please look at this. The badge shows the count when the queue drains!";
    let view = store.propose_requests(purpose, Some(prompt), Opening::Own(opening));
    assert_eq!(
        criterion_texts(&view),
        vec![
            "The badge shows the count when the queue drains.".to_string(),
            "The banner logs the error when the upload fails.".to_string(),
        ]
    );
    assert_eq!(
        slug_of(
            &view,
            AskOrigin::SessionOpening,
            "The badge shows the count when the queue drains"
        ),
        vec![REPEATS_ANOTHER]
    );
    for (origin, ask) in [
        (AskOrigin::ChangePurpose, purpose),
        (AskOrigin::OriginalPrompt, prompt),
        (AskOrigin::SessionOpening, opening),
    ] {
        words_hold(ask, &spans_of(&view, Some(origin)))
            .unwrap_or_else(|e| panic!("{origin:?} {ask:?}: {e}"));
    }
}

/// One request read twice — the Change points at the very event its
/// confirmed session opens with — is still said once, and nothing in it is
/// called a repeat of another request: it is not another request.
#[test]
fn one_request_read_twice_is_still_said_once() {
    let mut store = Store::new();
    let purpose = "The counter shows zero when the cart is empty.";
    let prompt = "Fix the checkout timer. The panel shows the report when the job ends.";
    let view = store.propose_requests(purpose, Some(prompt), Opening::SameAsPrompt);
    assert!(
        spans_of(&view, Some(AskOrigin::SessionOpening)).is_empty(),
        "the same event was read a second time: {:?}",
        spans_of(&view, Some(AskOrigin::SessionOpening))
    );
    assert!(notes_with(&view, REPEATS_ANOTHER).is_empty());
    for (origin, ask) in [
        (AskOrigin::ChangePurpose, purpose),
        (AskOrigin::OriginalPrompt, prompt),
    ] {
        words_hold(ask, &spans_of(&view, Some(origin)))
            .unwrap_or_else(|e| panic!("{origin:?} {ask:?}: {e}"));
    }
}

#[test]
fn the_new_note_has_a_sentence_in_the_copy_file() {
    let line = thruline_core::voice::PROPOSAL_LINES
        .iter()
        .find(|l| l.slug == REPEATS_ANOTHER)
        .unwrap_or_else(|| panic!("{REPEATS_ANOTHER} has no line in the copy file"));
    assert!(line
        .plain
        .starts_with("Kept as a note, not something to check:"));
    assert!(line.plain.contains("another request"), "{:?}", line.plain);
}

/// The multi-request corpus: the round-4 adversary's sentence generator,
/// spread over a purpose, a prompt and sometimes a confirmed session's
/// opening, with and without sentences they share, and sometimes with the
/// session opening on the prompt's own event. Per request, the words of the
/// criteria plus the words of the notes, in order, are that request's words,
/// and no cut breaks rule (b) or (c).
const MULTI_SEED: u64 = 0x5EED_F11C_2026_0929;
const MULTI_CORPUS_SIZE: usize = 600;

struct MultiAsk {
    purpose: String,
    prompt: String,
    opening: Option<String>,
    same_event: bool,
    shares: bool,
}

fn multi_corpus() -> Vec<MultiAsk> {
    let mut rng = Rng(MULTI_SEED);
    (0..MULTI_CORPUS_SIZE)
        .map(|_| {
            let purpose: Vec<String> = (0..1 + rng.next() % 2)
                .map(|_| adv_sentence(&mut rng))
                .collect();
            let mut prompt: Vec<String> = (0..1 + rng.next() % 2)
                .map(|_| adv_sentence(&mut rng))
                .collect();
            let mut shares = false;
            if rng.next().is_multiple_of(2) {
                let copy = purpose[rng.next() % purpose.len()].clone();
                let at = rng.next() % (prompt.len() + 1);
                prompt.insert(at, copy);
                shares = true;
            }
            let (opening, same_event) = match rng.next() % 3 {
                0 => (None, false),
                1 => {
                    let mut own: Vec<String> = (0..1 + rng.next() % 2)
                        .map(|_| adv_sentence(&mut rng))
                        .collect();
                    if rng.next().is_multiple_of(2) {
                        let pool: Vec<&String> = purpose.iter().chain(prompt.iter()).collect();
                        let copy = pool[rng.next() % pool.len()].clone();
                        let at = rng.next() % (own.len() + 1);
                        own.insert(at, copy);
                        shares = true;
                    }
                    (Some(own.join(" ")), false)
                }
                _ => (None, true),
            };
            MultiAsk {
                purpose: purpose.join(" "),
                prompt: prompt.join(" "),
                opening,
                same_event,
                shares,
            }
        })
        .collect()
}

#[test]
fn the_multi_request_corpus_keeps_every_word_of_every_request() {
    let asks = multi_corpus();
    assert_eq!(asks.len(), MULTI_CORPUS_SIZE);
    let mut store = Store::new();
    let mut failures: Vec<String> = Vec::new();
    let (mut sharing, mut same_event, mut three, mut another) = (0usize, 0usize, 0usize, 0usize);
    for (index, ask) in asks.iter().enumerate() {
        let opening = match (&ask.opening, ask.same_event) {
            (Some(text), _) => Opening::Own(text),
            (None, true) => Opening::SameAsPrompt,
            (None, false) => Opening::None,
        };
        let view = store.propose_requests(&ask.purpose, Some(&ask.prompt), opening);
        sharing += usize::from(ask.shares);
        same_event += usize::from(ask.same_event);
        three += usize::from(ask.opening.is_some());
        another += notes_with(&view, REPEATS_ANOTHER).len();
        let describe = || {
            format!(
                "#{index} purpose {:?} prompt {:?} opening {:?} same_event {}\n    criteria {:?}\n    notes {:?}",
                ask.purpose,
                ask.prompt,
                ask.opening,
                ask.same_event,
                criterion_texts(&view),
                note_texts(&view)
            )
        };
        let distinct: BTreeSet<String> = criterion_texts(&view)
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        if distinct.len() != view.criteria.len() {
            failures.push(format!("a criterion is proposed twice: {}", describe()));
            continue;
        }
        let mut requests = vec![
            (AskOrigin::ChangePurpose, ask.purpose.as_str()),
            (AskOrigin::OriginalPrompt, ask.prompt.as_str()),
        ];
        if let Some(text) = &ask.opening {
            requests.push((AskOrigin::SessionOpening, text.as_str()));
        } else if !spans_of(&view, Some(AskOrigin::SessionOpening)).is_empty() {
            failures.push(format!("one request was read twice: {}", describe()));
            continue;
        }
        for (origin, text) in requests {
            let spans = spans_of(&view, Some(origin));
            if let Err(e) = words_hold(text, &spans).and_then(|()| cuts_hold(text, &spans)) {
                failures.push(format!("{origin:?}: {e}\n  {}", describe()));
            }
        }
    }
    eprintln!(
        "QUAL-F11-LAUNCH-R6 multi-request corpus: seed {MULTI_SEED:#x}, {} changes ({sharing} sharing a sentence across requests, {three} with three requests, {same_event} with one request read twice), {another} notes repeating another request, {} failing",
        asks.len(),
        failures.len()
    );
    assert!(sharing > 0 && three > 0 && same_event > 0 && another > 0);
    assert!(
        failures.is_empty(),
        "{} of {} changes lose or duplicate words; the first ten:\n{}",
        failures.len(),
        asks.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
}
