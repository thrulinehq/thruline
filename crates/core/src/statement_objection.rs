//! C30 R-LEDGER — an objection: a person's "This is wrong…" on one of the
//! agent's statements, kept beside the statement.
//!
//! One row means: one person said one statement is wrong, with a reason. The
//! statement is named by its id (`<events.id>#<n>`, the id the thread view and
//! the statements route serve) and by the SHA-256 of its sentence AT THAT
//! MOMENT. There is no `change_id`: statements belong to threads whether or not
//! a Change tracks them.
//!
//! THE DIGEST IS THE SERVER'S. The caller hands [`object`] the [`ClaimRef`] the
//! project's own index gave for the id ([`crate::hq::Index::claim_sentence`]);
//! this module hashes that sentence. No client ever sends a digest or a
//! sentence. A later extractor that reads the same id differently cannot carry
//! the objection onto a different sentence: [`ObjectionItem::matches_sentence`]
//! is false when the stored digest no longer equals the digest of the sentence
//! the extractor gives now, and the screen says "the sentence changed since".
//!
//! WHAT AN OBJECTION IS NOT. It is words, never a result. The table has no
//! verdict column (the verdict is read at render time, so a state word is
//! never copied into a table that is not evidence), and nothing here is read by
//! `judge_thread`, a count, `whose_move`, readiness, an approval, the speaker
//! or the stop gate. Its one reader is the HQ thread view
//! ([`crate::hq::Index::thread_view`]), which serves it as `objections[]`
//! beside the unchanged verdict of the claim. Keeping one is free: nothing in
//! this module asks the licence gate.
//!
//! TAKING ONE BACK IS NOT BUILT. The table keeps the word `withdrawn` (C34
//! ruling Q16: the schema can say it, no route or control does yet), so a
//! reader here drops an objection that a later `withdrawn` row points at, and
//! no function here appends one.
//!
//! STORED AS TYPED, MASKED ON SERVE. The reason is kept exactly as typed, at
//! most [`MAX_REASON_CHARS`] characters; every reader passes the words through
//! [`crate::change::mask_text`], the Change layer's one masking helper, so this
//! module adds no masking call site of its own.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use ulid::Ulid;

use crate::chain::sha256_hex;
use crate::error::Result;
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The table, named once for the screens that cite a row by it.
pub const TABLE: &str = "statement_objections";

/// The longest reason, in characters. The table's own CHECK says the same; the
/// check here comes first so a person is told in a sentence.
pub const MAX_REASON_CHARS: usize = 2000;

/// What every surface that keeps or shows an objection says beside it.
pub const KEPT_BESIDE: &str = "Your objection is kept beside this statement. It does not \
     change what ThruLine says about it.";

/// The statement an objection is about, as the SERVER resolved it from the
/// project's own index: the id, the event it sits in, its place in that event,
/// and the sentence the extractor gives for it now. The client sends only the
/// id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRef {
    pub statement_id: String,
    pub event_id: String,
    pub ordinal: usize,
    pub sentence: String,
}

/// The event and ordinal a statement id names, or `None` for anything that is
/// not exactly `<event>#<n>` with `n` written the one way the table's CHECK
/// writes it (`"e1#01"` and `"e1#+1"` name nothing).
pub fn split_statement_id(statement_id: &str) -> Option<(&str, usize)> {
    let (event_id, ordinal) = statement_id.rsplit_once('#')?;
    if event_id.is_empty() {
        return None;
    }
    let n: usize = ordinal.parse().ok()?;
    (n.to_string() == ordinal).then_some((event_id, n))
}

/// The digest an objection keeps of a sentence: SHA-256 of its exact bytes,
/// lowercase hex.
pub fn sentence_sha256(sentence: &str) -> String {
    sha256_hex(sentence.as_bytes())
}

/// Why an objection was not kept. Nothing is written in any case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The statement id is not one this project's record serves, or its event
    /// is not in this store.
    NoSuchStatement,
    NoActor,
    /// A reason with nothing but whitespace in it.
    EmptyReason,
    /// More than [`MAX_REASON_CHARS`] characters; `chars` is how many.
    TooLong {
        chars: usize,
    },
}

impl Refusal {
    /// The stable code the route answers with.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoSuchStatement => "no_such_statement",
            Self::NoActor => "actor_required",
            Self::EmptyReason => "reason_required",
            Self::TooLong { .. } => "too_long",
        }
    }

    /// The sentence a person reads. Nothing was saved.
    pub fn plain(&self) -> String {
        match self {
            Self::NoSuchStatement => {
                "ThruLine has no record of this statement. Nothing was saved.".to_string()
            }
            Self::NoActor => {
                "Saying a statement is wrong needs a name on it. Nothing was saved.".to_string()
            }
            Self::EmptyReason => {
                "Say what is wrong with this statement. Nothing was saved.".to_string()
            }
            Self::TooLong { chars } => format!(
                "A reason can be at most {MAX_REASON_CHARS} characters; this one is {chars}. \
                 Nothing was saved."
            ),
        }
    }
}

/// Is this a reason that may be kept? The one place the question is answered,
/// for the route and for [`object`].
pub fn check_reason(reason: &str) -> std::result::Result<(), Refusal> {
    if reason.trim().is_empty() {
        return Err(Refusal::EmptyReason);
    }
    let chars = reason.chars().count();
    if chars > MAX_REASON_CHARS {
        return Err(Refusal::TooLong { chars });
    }
    Ok(())
}

/// One objection as served: masked, with who and when.
/// `matches_sentence` is false when the statement's sentence is no longer the
/// one the person objected to; the screen then says so and shows the objection
/// as about an earlier wording, never as about the new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectionItem {
    pub id: String,
    pub actor: String,
    pub at: String,
    pub reason: String,
    pub matches_sentence: bool,
}

/// One standing objection as stored, before it is served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub id: String,
    pub statement_id: String,
    pub statement_sha256: String,
    pub actor: String,
    pub reason: String,
    pub recorded_at: String,
}

impl Stored {
    /// The served form against the sentence the extractor gives now.
    pub fn served(&self, current_sentence: &str, record: &mut MaskRecord) -> ObjectionItem {
        ObjectionItem {
            id: self.id.clone(),
            actor: crate::change::mask_text(&self.actor, record),
            at: self.recorded_at.clone(),
            reason: crate::change::mask_text(&self.reason, record),
            matches_sentence: self.statement_sha256 == sentence_sha256(current_sentence),
        }
    }
}

fn mint() -> String {
    format!("obj_{}", Ulid::generate())
}

/// Keep that a person said `claim` is wrong, and why: one appended row. The
/// reason is stored exactly as given; the digest is of `claim.sentence`.
pub fn object(
    ledger: &Ledger,
    claim: &ClaimRef,
    actor: &str,
    reason: &str,
    at: &str,
) -> Result<std::result::Result<ObjectionItem, Refusal>> {
    let actor = actor.trim();
    if actor.is_empty() {
        return Ok(Err(Refusal::NoActor));
    }
    if let Err(refusal) = check_reason(reason) {
        return Ok(Err(refusal));
    }
    let conn = ledger.conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<std::result::Result<ObjectionItem, Refusal>> {
        let anchored = conn
            .query_row(
                "SELECT 1 FROM events WHERE id=?1",
                params![claim.event_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !anchored
            || split_statement_id(&claim.statement_id) != Some((&claim.event_id, claim.ordinal))
        {
            return Ok(Err(Refusal::NoSuchStatement));
        }
        let id = mint();
        let digest = sentence_sha256(&claim.sentence);
        conn.execute(
            "INSERT INTO statement_objections(id, act, statement_id, event_id, claim_ordinal,
                                              statement_sha256, actor, reason, recorded_at,
                                              withdraws)
             VALUES(?1,'objected',?2,?3,?4,?5,?6,?7,?8,NULL)",
            params![
                id,
                claim.statement_id,
                claim.event_id,
                claim.ordinal as i64,
                digest,
                actor,
                reason,
                at,
            ],
        )?;
        let stored = Stored {
            id,
            statement_id: claim.statement_id.clone(),
            statement_sha256: digest,
            actor: actor.to_string(),
            reason: reason.to_string(),
            recorded_at: at.to_string(),
        };
        let mut record = MaskRecord::default();
        Ok(Ok(stored.served(&claim.sentence, &mut record)))
    })();
    match outcome {
        Ok(result) => {
            conn.execute_batch("COMMIT")?;
            Ok(result)
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// Every standing objection, grouped by statement id, oldest first: an
/// `objected` row no later `withdrawn` row points at. One scan of a table a
/// human fills by hand, in the order the rows were appended.
pub fn standing(ledger: &Ledger) -> Result<BTreeMap<String, Vec<Stored>>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, act, statement_id, statement_sha256, actor, reason, recorded_at, withdraws
           FROM statement_objections ORDER BY rowid",
    )?;
    let mapped = statement.query_map([], |row| {
        Ok((
            Stored {
                id: row.get(0)?,
                statement_id: row.get(2)?,
                statement_sha256: row.get(3)?,
                actor: row.get(4)?,
                reason: row.get(5)?,
                recorded_at: row.get(6)?,
            },
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;
    let mut objected = Vec::new();
    let mut taken_back: BTreeSet<String> = BTreeSet::new();
    for row in mapped {
        let (stored, act, withdraws) = row?;
        match (act.as_str(), withdraws) {
            ("objected", _) => objected.push(stored),
            (_, Some(target)) => {
                taken_back.insert(target);
            }
            _ => {}
        }
    }
    let mut out: BTreeMap<String, Vec<Stored>> = BTreeMap::new();
    for stored in objected {
        if !taken_back.contains(&stored.id) {
            out.entry(stored.statement_id.clone())
                .or_default()
                .push(stored);
        }
    }
    Ok(out)
}

/// The id of the last row (of either act) appended for each event, keyed by
/// event id. A screen's cache keys on it: the index fingerprint does not move
/// when a person objects, because the objection lives in another table.
pub fn last_by_event(ledger: &Ledger) -> Result<BTreeMap<String, String>> {
    let mut statement = ledger
        .conn()
        .prepare("SELECT event_id, id FROM statement_objections ORDER BY rowid")?;
    let mapped = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = BTreeMap::new();
    for row in mapped {
        let (event_id, id) = row?;
        out.insert(event_id, id);
    }
    Ok(out)
}
