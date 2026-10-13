//! QUAL-F11-LAUNCH-R4 — when a sentence's quotation marks can be read two
//! ways, it is never cut: it is proposed whole (fail closed), proven by a
//! generated corpus, not by one more edge case.
//!
//! Round 3's adversary (`gates/adversary-QUAL-F11-LAUNCH-R3`, attacks A and
//! B) found that a plural possessive inside an open single quotation — "The
//! panel shows 'the drivers' report; Retry disabled' when the job ends." —
//! closed the quotation early, so the `;` after it cut the sentence and
//! "The panel shows 'the drivers' report." was proposed.
//!
//! # The rule these tests hold
//!
//! A **reading** of a stretch of text (a request, up to a line break that
//! starts a new sentence) gives every single quote mark (' ‘ ’) one role:
//! it opens a quotation, it closes the open one, or it is an apostrophe. A
//! reading is valid when every quotation it opens is closed and no mark is
//! left closing nothing. The roles a mark may take:
//!
//! - A mark with a letter or digit on both sides (don’t, it's) is an
//!   apostrophe, in every reading.
//! - While a single quotation is open: a mark that ends a word closes it —
//!   and, when it follows an `s` (the drivers' report), it may instead be a
//!   plural possessive, which leaves the quotation open. Any other mark is an
//!   apostrophe.
//! - While none is open: a mark that starts a word opens one — and, when the
//!   word is an elision ('til, ’tis, 'em, 'cause, ’90s …), it may instead be
//!   an apostrophe. A mark after an `s` that ends a word is a plural
//!   possessive. Any other mark closes nothing.
//!
//! A mark **has two readings** when valid readings of its stretch give it
//! different roles. A sentence is cut — at `;` or at `.` `!` `?` — only at a
//! position where every single mark before it in its stretch has exactly one
//! reading, and that position is inside no quotation. From the first mark with
//! two readings to the end of its stretch nothing is cut: the words stay
//! together in one proposal. The stop that ends the stretch cuts nothing off,
//! so it still ends the sentence, and the words are not a fragment. When no valid reading exists, the marks are read
//! left to right as round 3 read them, and a mark that could have been read
//! either way stops the cutting all the same.
//!
//! A candidate with no valid reading is a fragment and stays a note (round
//! 3's rule, which still covers text produced by a cut). A whole sentence
//! whose marks are odd in number only because of a possessive or an elision
//! has a valid reading, so it is proposed.
//!
//! # The generated corpus
//!
//! `the_generated_corpus_holds_the_rule` builds [`CORPUS_SIZE`] asks from the
//! blocks below with a linear congruential generator seeded with
//! [`SEED`]. Every choice is `next() % len` in the order the code makes it,
//! so a failing ask — printed with its index — can be rebuilt by hand. The
//! blocks: [`SUBJECTS`], [`SECOND_SUBJECTS`], [`CONDITIONS`], [`CONTENTS`],
//! the two quote families [`SINGLES`] and [`DOUBLES`] (every open × close
//! style), [`APOSTROPHES`], [`ELISION_MARKS`], [`TERMINATORS`] and the seven
//! sentence templates in `sentence()`; two or three sentences per ask.
//! After those, `QUAL-F11-LAUNCH-R5` adds [`SHORT_AND_REPEATED`] asks from the
//! same generator, each with a sentence from [`SHORTS`] (too short to check
//! on its own), a sentence repeated, or both; invariant (a) holds for them
//! with no exception.
//!
//! Every id is synthetic (`cccccccc-…`) and no fixture here holds private
//! content.

use std::collections::BTreeSet;

use thruline_core::change::{self, ChangeRequest, NewChange, RequestSource};
use thruline_core::change_propose::{self, ProfileEnvironment, ProposalView, ShapeRefusal};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000f0ae";

/// The generator's seed.
const SEED: u64 = 0x5155_414c_4631_3134;
/// How many asks the generator builds.
const CORPUS_SIZE: usize = 2400;

const SUBJECTS: [&str; 3] = ["the total", "the banner", "the badge"];
const SECOND_SUBJECTS: [&str; 3] = ["The counter", "The history", "The queue"];
const CONDITIONS: [&str; 4] = [
    "the cart is empty",
    "the job ends",
    "the form is sent",
    "a price is negative",
];
/// What goes between a pair of quote marks. `{a}` is an apostrophe (one of
/// [`APOSTROPHES`]) and `{e}` an elision mark (one of [`ELISION_MARKS`]).
const CONTENTS: [&str; 11] = [
    "delivered",
    "delivered; Retry disabled",
    "Saved. Retry later",
    "Stop! Retry now",
    "Sent? Check again",
    "the drivers{a} report; Retry disabled",
    "the drivers{a} report",
    "it{a}s saved; Retry later",
    "the users{a} cart; Retry the jobs{a} queue",
    "{e}til done; Retry later",
    "the drivers{a} report. Retry disabled",
];
const SINGLES: [char; 3] = ['\'', '‘', '’'];
const DOUBLES: [char; 3] = ['"', '“', '”'];
const APOSTROPHES: [char; 2] = ['\'', '’'];
const ELISION_MARKS: [char; 3] = ['\'', '‘', '’'];
const TERMINATORS: [&str; 3] = [".", "!", "?"];

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

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A quotation, or (one time in six) a plain word.
fn quotation(rng: &mut Rng) -> String {
    if rng.next().is_multiple_of(6) {
        return "zero".to_string();
    }
    let family = if rng.next().is_multiple_of(3) {
        DOUBLES
    } else {
        SINGLES
    };
    let open = rng.pick(&family);
    let close = rng.pick(&family);
    let a = rng.pick(&APOSTROPHES).to_string();
    let e = rng.pick(&ELISION_MARKS).to_string();
    let content = rng.pick(&CONTENTS).replace("{a}", &a).replace("{e}", &e);
    format!("{open}{content}{close}")
}

/// One sentence, in slot `k` of its ask (so no two sentences of one ask are
/// the same).
fn sentence(rng: &mut Rng, k: usize) -> String {
    let subject = SUBJECTS[k];
    let condition = rng.pick(&CONDITIONS);
    let term = rng.pick(&TERMINATORS);
    let template = rng.next() % 7;
    let q = quotation(rng);
    match template {
        0 => format!("When {condition}, {subject} shows {q}{term}"),
        1 => format!("{} shows {q} when {condition}{term}", capital(subject)),
        2 => format!(
            "{} shows {q} when {condition}; {} keeps zero{term}",
            capital(subject),
            SECOND_SUBJECTS[k]
        ),
        3 => {
            let a = rng.pick(&APOSTROPHES);
            format!("When the drivers{a} report is empty, {subject} shows {q}{term}")
        }
        4 => {
            let e = rng.pick(&ELISION_MARKS);
            format!("{} shows {q} {e}til {condition}{term}", capital(subject))
        }
        5 => {
            let q2 = quotation(rng);
            format!(
                "{} shows {q}; the service saying {q2} does not make it sent{term}",
                capital(subject)
            )
        }
        _ => format!("{q} is what {subject} shows when {condition}{term}"),
    }
}

/// Sentences too short to check on its own (`QUAL-F11-LAUNCH-R5`).
const SHORTS: [&str; 5] = ["Retry now", "Do it", "Ship it", "Why", "Thanks"];
/// How many more asks the generator builds after the first [`CORPUS_SIZE`],
/// each holding a short sentence, a repeated sentence, or both
/// (`QUAL-F11-LAUNCH-R5`). The first [`CORPUS_SIZE`] asks are unchanged.
const SHORT_AND_REPEATED: usize = 800;

/// An ask of two or three sentences, as round 4 built it.
fn plain_ask(rng: &mut Rng) -> Vec<String> {
    let count = 2 + rng.next() % 2;
    (0..count).map(|k| sentence(rng, k)).collect()
}

/// A round-4 ask with a short sentence (after `. ` or joined by `; `), a
/// sentence repeated (word for word, or in another case and stop), or both.
fn short_or_repeated_ask(rng: &mut Rng) -> String {
    let mut sentences = plain_ask(rng);
    let what = rng.next() % 3;
    if what != 1 {
        let short = rng.pick(&SHORTS);
        let term = rng.pick(&TERMINATORS);
        let at = rng.next() % (sentences.len() + 1);
        if at > 0 && rng.next().is_multiple_of(2) {
            // Joined to the sentence before it by a semicolon.
            let before = &mut sentences[at - 1];
            before.pop();
            before.push_str(&format!("; {short}{term}"));
        } else {
            sentences.insert(at, format!("{short}{term}"));
        }
    }
    if what != 0 {
        let from = rng.next() % sentences.len();
        let mut again = sentences[from].clone();
        if rng.next().is_multiple_of(3) {
            again = again.to_lowercase();
            again.pop();
            again.push('!');
        }
        let at = from + 1 + rng.next() % (sentences.len() - from);
        sentences.insert(at, again);
    }
    sentences.join(" ")
}

fn corpus() -> Vec<String> {
    let mut rng = Rng(SEED);
    let mut asks: Vec<String> = (0..CORPUS_SIZE)
        .map(|_| plain_ask(&mut rng).join(" "))
        .collect();
    asks.extend((0..SHORT_AND_REPEATED).map(|_| short_or_repeated_ask(&mut rng)));
    asks
}

// ---------------------------------------------------------------------------
// One store, many Changes
// ---------------------------------------------------------------------------

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
                 VALUES(?1, '/tmp/synthetic-f11-r4', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
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
}

fn criterion_texts(view: &ProposalView) -> Vec<String> {
    view.criteria.iter().map(|c| c.text.clone()).collect()
}

fn note_texts(view: &ProposalView) -> Vec<String> {
    view.notes.iter().map(|n| n.text.clone()).collect()
}

fn assert_proposed_whole(store: &mut Store, ask: &str) {
    let view = store.propose(ask);
    assert_eq!(
        criterion_texts(&view),
        vec![ask.to_string()],
        "{ask:?} must be proposed whole; notes: {:?}",
        note_texts(&view)
    );
    assert!(
        view.notes.is_empty(),
        "{ask:?}: nothing is left over: {:?}",
        note_texts(&view)
    );
}

// ---------------------------------------------------------------------------
// The oracle: every reading, by brute force
// ---------------------------------------------------------------------------

const ELISIONS: [&str; 10] = [
    "til", "tis", "twas", "em", "cause", "bout", "round", "n", "nuff", "cos",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    Open,
    Close(usize),
    Apostrophe,
    Unmatched,
}

struct Mark {
    at: usize,
    opens: bool,
    closes: bool,
    possessive: bool,
    elision: bool,
}

fn single_marks(text: &str) -> Vec<Mark> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut marks = Vec::new();
    for (i, &(at, ch)) in chars.iter().enumerate() {
        if !matches!(ch, '\'' | '‘' | '’') {
            continue;
        }
        let prev = i.checked_sub(1).map(|j| chars[j].1);
        let next = chars.get(i + 1).map(|(_, c)| *c);
        if prev.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric) {
            continue; // an apostrophe inside a word, in every reading
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
        marks.push(Mark {
            at,
            opens,
            closes,
            possessive,
            elision,
        });
    }
    marks
}

/// Every reading of `text`'s single marks: each is the role of every mark, in
/// order.
fn all_readings(marks: &[Mark]) -> Vec<Vec<Role>> {
    fn walk(
        marks: &[Mark],
        i: usize,
        open: Option<usize>,
        so_far: &mut Vec<Role>,
        out: &mut Vec<Vec<Role>>,
    ) {
        if i == marks.len() {
            out.push(so_far.clone());
            return;
        }
        let m = &marks[i];
        let mut roles: Vec<(Role, Option<usize>)> = Vec::new();
        match open {
            Some(from) => {
                if m.closes {
                    roles.push((Role::Close(from), None));
                    if m.possessive {
                        roles.push((Role::Apostrophe, open));
                    }
                } else {
                    roles.push((Role::Apostrophe, open));
                }
            }
            None => {
                if m.opens {
                    roles.push((Role::Open, Some(m.at)));
                    if m.elision {
                        roles.push((Role::Apostrophe, None));
                    }
                } else if m.possessive {
                    roles.push((Role::Apostrophe, None));
                } else {
                    roles.push((Role::Unmatched, None));
                }
            }
        }
        for (role, after) in roles {
            so_far.push(role);
            walk(marks, i + 1, after, so_far, out);
            so_far.pop();
        }
    }
    let mut out = Vec::new();
    walk(marks, 0, None, &mut Vec::new(), &mut out);
    out
}

fn is_valid(reading: &[Role]) -> bool {
    let mut open = false;
    for role in reading {
        match role {
            Role::Open => open = true,
            Role::Close(_) => open = false,
            Role::Unmatched => return false,
            Role::Apostrophe => {}
        }
    }
    !open
}

/// Is a double quotation open at `at`? Double marks have one reading each
/// (round 3's rule, unchanged).
fn double_open_at(text: &str, at: usize) -> bool {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut open = false;
    for (i, &(index, ch)) in chars.iter().enumerate() {
        if index >= at {
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

/// What went wrong with one ask, if anything.
fn check(ask: &str, view: &ProposalView) -> Result<usize, String> {
    // (a) the words of the criteria and the notes, in the ask's order, are the
    // words of the ask.
    let mut spans: Vec<(usize, usize, bool, String)> = view
        .criteria
        .iter()
        .map(|c| (c.source.offset, c.source.length, true, c.text.clone()))
        .chain(
            view.notes
                .iter()
                .map(|n| (n.source.offset, n.source.length, false, n.text.clone())),
        )
        .collect();
    spans.sort();
    let words = |text: &str| -> Vec<String> {
        text.split_whitespace()
            .map(|w| w.trim_end_matches(['.', ';', '!', '?', ',']).to_string())
            .filter(|w| !w.is_empty())
            .collect()
    };
    let kept: Vec<String> = spans.iter().flat_map(|(_, _, _, t)| words(t)).collect();
    if kept != words(ask) {
        return Err(format!("(a) words differ: kept {kept:?}"));
    }
    // (b), (c) at every cut. A blank line ends a stretch, and marks are read
    // one stretch at a time; the oracle knows no other line break.
    if ask.replace("\n\n", "").contains('\n') {
        return Err("the oracle reads blank lines only".to_string());
    }
    let mut stretches: Vec<(usize, usize)> = Vec::new();
    let mut from = 0usize;
    for (at, _) in ask.match_indices("\n\n") {
        stretches.push((from, at));
        from = at + 2;
    }
    stretches.push((from, ask.len()));
    for pair in spans.windows(2) {
        let (from, len, is_criterion, _) = &pair[0];
        let (next_from, _, _, _) = &pair[1];
        let gap = &ask[from + len..*next_from];
        let Some(cut) = gap
            .char_indices()
            .find(|(_, c)| matches!(c, '.' | ';' | '!' | '?'))
            .map(|(i, _)| from + len + i)
        else {
            return Err(format!("a cut with no mark between {gap:?}"));
        };
        let &(s0, s1) = stretches
            .iter()
            .find(|(a, b)| *a <= cut && cut < *b)
            .ok_or("a cut outside every stretch")?;
        let stretch = &ask[s0..s1];
        let marks = single_marks(stretch);
        let every = all_readings(&marks);
        let valid: Vec<Vec<Role>> = every.iter().filter(|r| is_valid(r)).cloned().collect();
        let readings = if valid.is_empty() { every } else { valid };
        // The stop that ends a stretch cuts nothing off: item 2 governs the
        // cuts that leave words after them.
        let ends_its_stretch = ask[cut + 1..s1].trim().is_empty();
        let before: Vec<usize> = (0..marks.len())
            .filter(|i| s0 + marks[*i].at < cut)
            .collect();
        if !ends_its_stretch {
            for i in &before {
                let roles: BTreeSet<Role> = readings.iter().map(|r| r[*i]).collect();
                if roles.len() != 1 {
                    return Err(format!(
                        "(c) cut at byte {cut} after the mark at byte {} with readings {roles:?}",
                        s0 + marks[*i].at
                    ));
                }
            }
        }
        if *is_criterion {
            for reading in &readings {
                let mut open = false;
                for i in &before {
                    match reading[*i] {
                        Role::Open => open = true,
                        Role::Close(_) => open = false,
                        _ => {}
                    }
                }
                if open || double_open_at(stretch, cut - s0) {
                    return Err(format!(
                        "(b) a proposed criterion ends at byte {cut}, inside a quotation"
                    ));
                }
            }
        }
    }
    Ok(spans.len().saturating_sub(1))
}

// ---------------------------------------------------------------------------
// Round 3's adversary, re-created (red at 52ffd138)
// ---------------------------------------------------------------------------

/// Attack A: a plural possessive inside an open straight quotation.
#[test]
fn attack_a_a_possessive_inside_an_open_quotation_does_not_cut_the_sentence() {
    let mut store = Store::new();
    assert_proposed_whole(
        &mut store,
        "The panel shows 'the drivers' report; Retry disabled' when the job ends.",
    );
}

/// Attack B: the same, typographic.
#[test]
fn attack_b_a_typographic_possessive_inside_an_open_quotation_does_not_cut_the_sentence() {
    let mut store = Store::new();
    assert_proposed_whole(
        &mut store,
        "The panel shows ‘the drivers’ report; Retry disabled’ when the job ends.",
    );
}

/// The adversary's control: a possessive with nothing open.
#[test]
fn a_possessive_with_nothing_open_is_still_an_apostrophe() {
    let mut store = Store::new();
    assert_proposed_whole(
        &mut store,
        "When the drivers' report is empty, the total shows zero.",
    );
}

// ---------------------------------------------------------------------------
// The rule
// ---------------------------------------------------------------------------

/// Both readings of the possessive are valid ("the jobs'" may close the
/// quotation too), so nothing after it is cut — not the `;`, and not the
/// full stop before the next sentence either. The words stay together.
#[test]
fn a_mark_with_two_readings_stops_every_later_cut_in_its_stretch() {
    let mut store = Store::new();
    let ask = "The panel shows 'the drivers' report; Retry the jobs' when the job ends. The total shows zero.";
    let view = store.propose(ask);
    let all: Vec<String> = criterion_texts(&view)
        .into_iter()
        .map(|t| t.trim_end_matches('.').to_string())
        .chain(note_texts(&view))
        .collect();
    assert_eq!(
        all,
        vec![ask.trim_end_matches('.').to_string()],
        "kept together, as one proposal or one note"
    );
    check(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
}

/// A cut before the first mark with two readings is still made.
#[test]
fn a_cut_before_the_ambiguous_mark_is_still_made() {
    let mut store = Store::new();
    let view = store.propose(
        "When the cart is empty, the total shows zero. The panel shows 'the drivers' report; Retry the jobs' when the job ends.",
    );
    assert_eq!(
        criterion_texts(&view).first().map(String::as_str),
        Some("When the cart is empty, the total shows zero.")
    );
}

/// The words kept together after a mark with two readings still end on the
/// person's own full stop: the stop that ends the request is not a cut that
/// could leave anything behind, so the words are a whole sentence, not a
/// fragment. (At 433940fb the held stop left them "trailing off", and they
/// were kept as a fragment note.)
#[test]
fn the_words_kept_together_end_on_the_persons_own_stop() {
    let mut store = Store::new();
    for ask in [
        "When the cart is empty, the total shows zero. The panel shows 'the drivers' report; Retry the jobs' when the job ends.",
        "When the cart is empty, the total shows zero. The total shows zero ’til the cart is empty! The banner shows ’Stop! Retry now‘ ‘til the form is sent!",
        "When the cart is empty, the total shows zero. The panel shows 'the drivers' report; Retry the jobs' when the job ends.\n\nWhen a price is negative, the form shows an error.",
    ] {
        let view = store.propose(ask);
        assert!(
            view.notes.iter().all(|n| n.refusal != ShapeRefusal::Fragment),
            "{ask:?}: kept as a fragment: {:?}",
            note_texts(&view)
        );
        check(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
    }
}

/// A whole sentence whose marks are odd in number only because of an elision
/// is the person's own sentence: it is proposed. At 52ffd138 the ’ read as a
/// quotation that never closed, and the sentence was a note.
#[test]
fn a_whole_sentence_with_an_elision_is_proposed() {
    let mut store = Store::new();
    for ask in [
        "When the cart is empty, the total shows zero ’til the next order.",
        "When the cart is empty, the total shows zero 'til the next order.",
        "When the cart is empty, the badge shows the ’90s price.",
    ] {
        assert_proposed_whole(&mut store, ask);
    }
}

/// A whole sentence with a possessive inside a quotation it closes later.
#[test]
fn a_whole_sentence_with_a_possessive_inside_a_quotation_is_proposed() {
    let mut store = Store::new();
    for ask in [
        "The page shows 'the drivers' report' when the job ends.",
        "The page shows ‘the drivers’ report’ when the job ends.",
        "When the users' cart is empty, the page shows 'the drivers' report' again.",
    ] {
        assert_proposed_whole(&mut store, ask);
    }
}

/// Round 3's rule still covers fragments: a piece a cut produced, holding a
/// quotation it never closes, stays a note.
#[test]
fn a_fragment_with_an_unmatched_mark_is_still_a_note() {
    let mut store = Store::new();
    let view = store.propose("The service shows 'delivered; Retry disabled when the job ends.");
    assert!(
        view.notes.iter().any(
            |n| n.refusal == ShapeRefusal::Fragment && n.text == "The service shows 'delivered"
        ),
        "{:?}",
        note_texts(&view)
    );
    assert!(criterion_texts(&view).iter().all(|t| !t.contains('\'')));
}

/// A sentence that starts with a quotation keeps its opening mark: nothing
/// the person wrote is dropped.
#[test]
fn a_sentence_that_starts_with_a_quotation_keeps_its_opening_mark() {
    let mut store = Store::new();
    let ask = "When the cart is empty, the total shows zero. 'Saved' is what the banner shows when the form is sent.";
    let view = store.propose(ask);
    check(ask, &view).unwrap_or_else(|e| panic!("{ask:?}: {e}"));
}

// ---------------------------------------------------------------------------
// Rounds 1–3, pinned
// ---------------------------------------------------------------------------

const Q02_PURPOSE: &str = "Two retry requests for the same delivery do not enqueue duplicate work. A delivery that was still waiting when the service was stopped is still exactly one delivery after it starts again, and it is attempted again. A delivery that has exhausted its attempts is shown as failed; the service saying 'sent' does not make it sent.";
const WALK_ASK: &str = "Add a cart total function. When the cart is empty, the total shows zero. The total shows the sum of the prices.";

#[test]
fn every_earlier_rounds_case_gives_the_result_it_pinned() {
    let mut store = Store::new();
    // Round 1: C1–C3 of Q02, and the walk.
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
    check(Q02_PURPOSE, &view).unwrap_or_else(|e| panic!("Q02: {e}"));
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
    let view = store.propose(
        "When the cart is empty, the total shows zero; When a price is negative, the form shows an error.",
    );
    assert_eq!(
        criterion_texts(&view),
        vec![
            "When the cart is empty, the total shows zero.".to_string(),
            "When a price is negative, the form shows an error.".to_string(),
        ]
    );
    // Rounds 2 and 3: proposed whole.
    for ask in [
        "The service shows 'delivered; Retry disabled' when the job ends.",
        "The service shows \"delivered; Retry disabled\" when the job ends.",
        "The service shows ‘delivered; Retry disabled’ when the job ends.",
        "The service shows “delivered; Retry disabled” when the job ends.",
        "The service shows `delivered; Retry disabled` when the job ends.",
        "The page shows 'Saved. Retry later' when the job fails.",
        "The page shows \"Stop! Retry now\" when the job fails.",
        "The page shows “Sent? Check again” when the job fails.",
        "When the user's cart is empty, the total shows zero.",
        "When the users' cart is empty, the total shows zero.",
        "When the user’s cart is empty, the total shows zero.",
        "The service shows 'delivered; Retry disabled’ when the job ends.",
        "The service shows \"delivered; Retry disabled” when the job ends.",
        "The service shows ‘delivered; Retry disabled' when the job ends.",
        "The panel shows 'delivered; The retry counter stores zero’ after the job ends.",
        "The page shows 'the service’s state; Retry later' when the job fails.",
        "The page shows ‘it's saved; Retry later’ when the job fails.",
    ] {
        assert_proposed_whole(&mut store, ask);
    }
    // Rounds 2 and 3: kept whole as a fragment note, nothing proposed.
    for ask in [
        "The page shows 'Saved when the form is sent.",
        "The page shows \"Saved when the form is sent.",
        "The page shows ‘Saved when the form is sent.",
        "The page shows “Saved when the form is sent.",
        "The page shows `Saved when the form is sent.",
        "The service shows 'delivered when the job ends” twice.",
        "The service shows “delivered when the job ends’ twice.",
        "The retry counter stores zero’ after the job ends.",
        "The retry counter stores zero' after the job ends.",
        "The retry counter stores zero” after the job ends.",
        "The retry counter stores zero\" after the job ends.",
        "The retry counter stores ‘ zero after the job ends.",
        "The retry counter stores ’ zero after the job ends.",
    ] {
        let view = store.propose(ask);
        assert!(
            view.criteria.is_empty(),
            "{ask:?}: {:?}",
            criterion_texts(&view)
        );
        let note = view
            .notes
            .iter()
            .find(|n| n.refusal == ShapeRefusal::Fragment)
            .unwrap_or_else(|| panic!("{ask:?}: {:?}", note_texts(&view)));
        assert_eq!(format!("{}.", note.text), ask);
    }
}

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

#[test]
fn the_generated_corpus_holds_the_rule() {
    let asks = corpus();
    assert!(asks.len() >= 2000);
    let distinct: BTreeSet<&String> = asks.iter().collect();
    let mut store = Store::new();
    let mut failures = Vec::new();
    let (mut cuts, mut criteria, mut notes) = (0usize, 0usize, 0usize);
    let (mut short, mut repeats) = (0usize, 0usize);
    for (index, ask) in asks.iter().enumerate() {
        let view = store.propose(ask);
        criteria += view.criteria.len();
        notes += view.notes.len();
        short += view
            .notes
            .iter()
            .filter(|n| n.reason_slug == "note_too_short")
            .count();
        repeats += view
            .notes
            .iter()
            .filter(|n| n.reason_slug == "note_repeats_an_earlier_line")
            .count();
        match check(ask, &view) {
            Ok(made) => cuts += made,
            Err(what) => failures.push(format!(
                "#{index} {ask:?}: {what}\n    criteria {:?}\n    notes {:?}",
                criterion_texts(&view),
                note_texts(&view)
            )),
        }
    }
    eprintln!(
        "QUAL-F11-LAUNCH-R4/R5 corpus: seed {SEED:#x}, {} asks ({} distinct), {cuts} cuts checked, {criteria} criteria, {notes} notes ({short} too short, {repeats} repeats), {} failing",
        asks.len(),
        distinct.len(),
        failures.len()
    );
    assert!(cuts > 1000, "the corpus exercises cuts: {cuts}");
    assert!(
        short > 100 && repeats > 100,
        "the corpus exercises short and repeated sentences: {short}, {repeats}"
    );
    assert!(
        failures.is_empty(),
        "{} of {} asks break the rule; the first ten:\n{}",
        failures.len(),
        asks.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
