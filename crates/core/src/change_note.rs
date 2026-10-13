//! C30 R-LEDGER — a person's note on a Change, and the Approve note.
//!
//! One row means: one person kept these words on one Change. When
//! `decision_id` is set the words were kept *with that approval or
//! withdrawal*, in the same transaction as the decision row
//! ([`crate::change_decision::approve`] / [`crate::change_decision::withdraw`]
//! call [`insert`] inside their own `BEGIN IMMEDIATE`), so the act and its note
//! are recorded together or not at all. The Approve note *is* a Change note
//! that names a decision; there is no second table.
//!
//! WHAT A NOTE IS NOT. It is words, never a result. Nothing here is read by a
//! verdict, by coverage, by readiness, by a decision's standing or by
//! `change_decision::read`'s choice of which approval stands; the decision
//! file `.thruline/decisions/<version>.json` does not carry it and stays
//! schema 3 (C34 ruling Q11); and it is never quoted to the agent. Adding a
//! note is free: the one paid act is approving a version, and nothing in this
//! module asks `licence_cmd::entitled`.
//!
//! STORED AS TYPED, MASKED ON SERVE. The table keeps the person's words
//! exactly as typed. Every reader in this module ([`read`], and the `note` that
//! [`crate::change_decision`] derives onto a decision row) passes them through
//! [`crate::change::mask_text`], the Change layer's one masking helper.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use ulid::Ulid;

use crate::error::Result;
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The table, named once for the screens that cite a row by it.
pub const TABLE: &str = "change_notes";

/// The longest note, in characters. The table's own CHECK says the same; the
/// check here comes first so a person is told in a sentence instead of
/// reading a constraint name.
pub const MAX_NOTE_CHARS: usize = 2000;

/// Why a note was not kept. Nothing is written in either case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteProblem {
    /// Nothing but whitespace.
    Empty,
    /// More than [`MAX_NOTE_CHARS`] characters; `chars` is how many.
    TooLong { chars: usize },
}

impl NoteProblem {
    /// The sentence a person reads. Nothing was saved in either case.
    pub fn plain(self) -> String {
        match self {
            NoteProblem::Empty => "A note has to say something. Nothing was saved.".to_string(),
            NoteProblem::TooLong { chars } => format!(
                "A note can be at most {MAX_NOTE_CHARS} characters; this one is {chars}. \
                 Nothing was saved."
            ),
        }
    }
}

/// Is this a note that may be kept? The one place the question is answered,
/// for the route, the terminal door and the approval that carries a note.
pub fn check(body: &str) -> std::result::Result<(), NoteProblem> {
    if body.trim().is_empty() {
        return Err(NoteProblem::Empty);
    }
    let chars = body.chars().count();
    if chars > MAX_NOTE_CHARS {
        return Err(NoteProblem::TooLong { chars });
    }
    Ok(())
}

/// One note, as served: masked, with who and when. Every field is stored, so
/// every field is always present (`decision_id` is `null` for a note added
/// from the Brief).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteView {
    pub id: String,
    pub actor: String,
    pub body: String,
    pub recorded_at: String,
    /// The approval or withdrawal this note was kept with.
    pub decision_id: Option<String>,
}

/// Why a note added from outside an approval was not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NoSuchChange,
    NoActor,
    Note(NoteProblem),
}

impl Refusal {
    /// The sentence a person reads. Nothing was saved in any case.
    pub fn plain(self, change_id: &str) -> String {
        match self {
            Refusal::NoSuchChange => format!(
                "No Change with id {change_id:?} is recorded in this store. Nothing was saved."
            ),
            Refusal::NoActor => {
                "A note has a name on it: say who is writing it with --actor <name>. \
                 Nothing was saved."
                    .to_string()
            }
            Refusal::Note(problem) => problem.plain(),
        }
    }
}

fn mint() -> String {
    format!("cn_{}", Ulid::generate())
}

/// Keep one note on a Change, with no decision: one appended row.
///
/// `at` is the time it is recorded. The words are stored exactly as given.
pub fn add(
    ledger: &Ledger,
    change_id: &str,
    actor: &str,
    body: &str,
    at: &str,
) -> Result<std::result::Result<NoteView, Refusal>> {
    let exists = ledger
        .conn()
        .query_row(
            "SELECT 1 FROM changes WHERE id=?1",
            params![change_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(Err(Refusal::NoSuchChange));
    }
    let actor = actor.trim();
    if actor.is_empty() {
        return Ok(Err(Refusal::NoActor));
    }
    if let Err(problem) = check(body) {
        return Ok(Err(Refusal::Note(problem)));
    }
    let id = mint();
    insert(ledger, &id, change_id, None, actor, body, at)?;
    Ok(Ok(view(id, actor, body, at, None)))
}

/// Append one `change_notes` row. The caller has checked the words with
/// [`check`]; the table's own constraints stay behind it as a second line.
///
/// Called by the approval writer inside its own transaction, with
/// `decision_id` naming the row it has just appended.
pub(crate) fn insert(
    ledger: &Ledger,
    id: &str,
    change_id: &str,
    decision_id: Option<&str>,
    actor: &str,
    body: &str,
    at: &str,
) -> Result<()> {
    ledger.conn().execute(
        "INSERT INTO change_notes(id, change_id, decision_id, actor, body, recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, change_id, decision_id, actor, body, at],
    )?;
    Ok(())
}

/// A note kept with an approval or withdrawal, appended inside the act.
/// Returns the row's id.
pub(crate) fn insert_with_decision(
    ledger: &Ledger,
    change_id: &str,
    decision_id: &str,
    actor: &str,
    body: &str,
    at: &str,
) -> Result<String> {
    let id = mint();
    insert(ledger, &id, change_id, Some(decision_id), actor, body, at)?;
    Ok(id)
}

/// A note's words as they are served: masked, by the Change layer's one
/// masking helper ([`crate::change::mask_text`], already classified in
/// `redact::MASK_CALL_SITES`), so this module adds no masking call site of its
/// own.
pub(crate) fn masked(body: &str) -> String {
    let mut record = MaskRecord::default();
    crate::change::mask_text(body, &mut record)
}

fn view(id: String, actor: &str, body: &str, at: &str, decision_id: Option<String>) -> NoteView {
    let mut record = MaskRecord::default();
    view_masked(id, actor, body, at, decision_id, &mut record)
}

fn view_masked(
    id: String,
    actor: &str,
    body: &str,
    at: &str,
    decision_id: Option<String>,
    record: &mut MaskRecord,
) -> NoteView {
    NoteView {
        id,
        actor: crate::change::mask_text(actor, record),
        body: crate::change::mask_text(body, record),
        recorded_at: at.to_string(),
        decision_id,
    }
}

/// Every note kept on one Change, in the order they were appended, masked.
/// What the masker did is merged into `record`, so the Change's own
/// `masking` treatment accounts for these words too.
pub fn read(ledger: &Ledger, change_id: &str, record: &mut MaskRecord) -> Result<Vec<NoteView>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, actor, body, recorded_at, decision_id
           FROM change_notes WHERE change_id=?1 ORDER BY recorded_at, rowid",
    )?;
    let mapped = statement.query_map(params![change_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in mapped {
        let (id, actor, body, recorded_at, decision_id) = row?;
        out.push(view_masked(
            id,
            &actor,
            &body,
            &recorded_at,
            decision_id,
            record,
        ));
    }
    Ok(out)
}

/// The words kept with each decision of one Change, masked, by decision id.
/// A decision carries at most one note: its only writer is the act itself.
pub(crate) fn by_decision(
    ledger: &Ledger,
    change_id: &str,
) -> Result<std::collections::BTreeMap<String, String>> {
    let mut statement = ledger.conn().prepare(
        "SELECT decision_id, body FROM change_notes
          WHERE change_id=?1 AND decision_id IS NOT NULL ORDER BY rowid",
    )?;
    let mapped = statement.query_map(params![change_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = std::collections::BTreeMap::new();
    for row in mapped {
        let (decision_id, body) = row?;
        out.insert(decision_id, masked(&body));
    }
    Ok(out)
}
