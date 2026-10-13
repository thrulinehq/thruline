//! C30 R-LEDGER — telling the agent: what a person copied for it, and what
//! they said they pasted.
//!
//! One row means: a person copied a message for the agent (with its text), said
//! they pasted it, or took that back. It is about a THREAD of work — the
//! Inbox's "Waiting on the agent" rows are threads, tracked or not — and it
//! names the Change when one tracks the thread. `thread_event_id` is the
//! thread's anchor (its first work prompt), so a re-keyed thread can be
//! re-joined and the act has a receipt path to raw JSONL.
//!
//! NOTHING IS SENT. The table has three acts — `copied`, `marked_pasted`,
//! `unmarked_pasted` — and no act that could say a message was delivered;
//! ThruLine writes to the clipboard on the person's click and to nothing else.
//! Every sentence this module hands a screen says so ([`NOTHING_SENT`]).
//!
//! WHAT AN ACT IS NOT. It is words and a mark, never a result. It changes no
//! `whose_move`, no verdict, no count, no readiness and no approval; the
//! engine does not read this table. Its one reader is the HQ project view
//! (`told_agent` on a thread, [`crate::hq::Index::view_told`]), which files a
//! thread under "Waiting on the agent" while the last act is a copy or a mark
//! and no later work has been recorded. Writing it is free: nothing here asks
//! `licence_cmd::entitled`.
//!
//! STORED AS WRITTEN, MASKED ON SERVE (C34 ruling Q12). The message is kept
//! exactly as the person left it in the draft, at most [`MAX_MESSAGE_CHARS`]
//! characters, with the SHA-256 of those exact bytes — which the server
//! computes, never the client. Every reader here passes the words through
//! [`crate::change::mask_text`], the Change layer's one masking helper, so
//! this module adds no masking call site of its own.

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use ulid::Ulid;

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The table, named once for the screens that cite a row by it.
pub const TABLE: &str = "change_agent_messages";

/// The longest message, in characters. The table's own CHECK says the same;
/// the check here comes first so a person is told in a sentence.
pub const MAX_MESSAGE_CHARS: usize = 8000;

/// What every surface that shows a copy or a mark says beside it. The table
/// has no act that could say otherwise.
pub const NOTHING_SENT: &str = "Nothing was sent. ThruLine only keeps that you copied this \
     and, if you say so, that you pasted it.";

/// The three acts, and there is no fourth: in particular none that could mean
/// a message was delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Act {
    Copied,
    MarkedPasted,
    UnmarkedPasted,
}

impl Act {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copied => "copied",
            Self::MarkedPasted => "marked_pasted",
            Self::UnmarkedPasted => "unmarked_pasted",
        }
    }

    /// The act a request names, or `None` for any other word.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "copied" => Some(Self::Copied),
            "marked_pasted" => Some(Self::MarkedPasted),
            "unmarked_pasted" => Some(Self::UnmarkedPasted),
            _ => None,
        }
    }

    fn parse_stored(value: &str) -> Result<Self> {
        Self::parse(value).ok_or_else(|| {
            CoreError::Integrity(format!(
                "a message-for-the-agent act must be copied, marked_pasted or \
                 unmarked_pasted, not {value:?}"
            ))
        })
    }
}

/// The thread an act is about, as the SERVER resolved it — never a thing the
/// client sends: the anchor event (the thread's first work prompt), the
/// version the thread is at, and the Change that tracks the thread, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub thread_id: String,
    pub thread_event_id: String,
    pub change_id: Option<String>,
    pub version_id: Option<String>,
}

/// Why an act was not kept. Nothing is written in any case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The anchor event is not in this store.
    NoSuchThread,
    NoActor,
    /// A copy with nothing in it.
    EmptyMessage,
    /// More than [`MAX_MESSAGE_CHARS`] characters; `chars` is how many.
    TooLong {
        chars: usize,
    },
    /// A pasted mark with no copy on this thread to refer to.
    NoCopyToMark,
    /// The copy already carries a standing pasted mark; `mark` is its id.
    AlreadyMarked {
        mark: String,
    },
    /// An undo with no standing mark to undo.
    NoMarkToUndo,
}

impl Refusal {
    /// The stable code the route answers with.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoSuchThread => "no_such_thread",
            Self::NoActor => "actor_required",
            Self::EmptyMessage => "message_required",
            Self::TooLong { .. } => "too_long",
            Self::NoCopyToMark => "no_copy_to_mark",
            Self::AlreadyMarked { .. } => "already_marked",
            Self::NoMarkToUndo => "no_mark_to_undo",
        }
    }

    /// The sentence a person reads. Nothing was saved, and nothing was sent.
    pub fn plain(&self) -> String {
        match self {
            Self::NoSuchThread => {
                "ThruLine has no record of this piece of work. Nothing was saved.".to_string()
            }
            Self::NoActor => {
                "Saying you copied or pasted something needs a name on it. Nothing was saved."
                    .to_string()
            }
            Self::EmptyMessage => {
                "There is no message to keep: the draft is empty. Nothing was saved.".to_string()
            }
            Self::TooLong { chars } => format!(
                "A message for the agent can be at most {MAX_MESSAGE_CHARS} characters; this one \
                 is {chars}. Nothing was saved."
            ),
            Self::NoCopyToMark => {
                "There is no copied message on this piece of work to mark as pasted. Nothing \
                 was saved."
                    .to_string()
            }
            Self::AlreadyMarked { .. } => {
                "That copy is already marked as pasted. Nothing was saved.".to_string()
            }
            Self::NoMarkToUndo => {
                "There is no \"pasted\" mark on this piece of work to undo. Nothing was saved."
                    .to_string()
            }
        }
    }
}

/// Is this a message that may be kept? The one place the question is
/// answered, for the route and for [`copied`].
pub fn check_message(message: &str) -> std::result::Result<(), Refusal> {
    if message.trim().is_empty() {
        return Err(Refusal::EmptyMessage);
    }
    let chars = message.chars().count();
    if chars > MAX_MESSAGE_CHARS {
        return Err(Refusal::TooLong { chars });
    }
    Ok(())
}

/// One act as served: masked, with who and when. `message` is `null` except on
/// a copy; `message_sha256` is the digest of the exact bytes stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActView {
    pub id: String,
    pub act: Act,
    pub thread_id: String,
    pub thread_event_id: String,
    pub change_id: Option<String>,
    pub version_id: Option<String>,
    pub actor: String,
    pub recorded_at: String,
    pub refers_to: Option<String>,
    pub message: Option<String>,
    pub message_sha256: Option<String>,
}

/// The last act on one thread, before `later_work` is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToldAct {
    pub last_act: Act,
    pub at: String,
    pub version_id: Option<String>,
    pub act_id: String,
}

impl ToldAct {
    /// The served form, once the index has said whether any later row of the
    /// thread exists. `later_work` is derived, never stored.
    pub fn served(&self, later_work: bool) -> ToldAgent {
        ToldAgent {
            last_act: self.last_act,
            at: self.at.clone(),
            version_id: self.version_id.clone(),
            act_id: self.act_id.clone(),
            later_work,
        }
    }
}

/// What the project view serves per thread that has an act: the last act, when,
/// at which version, which row, and whether the thread has since had any work
/// recorded. No text: the message is not part of this answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToldAgent {
    pub last_act: Act,
    pub at: String,
    pub version_id: Option<String>,
    pub act_id: String,
    pub later_work: bool,
}

#[derive(Debug, Clone)]
struct Stored {
    id: String,
    act: Act,
    refers_to: Option<String>,
}

fn mint() -> String {
    format!("cam_{}", Ulid::generate())
}

/// Keep that a person copied `message` for the agent: one appended row. The
/// words are stored exactly as given, with the SHA-256 of those bytes.
pub fn copied(
    ledger: &Ledger,
    anchor: &Anchor,
    actor: &str,
    message: &str,
    at: &str,
) -> Result<std::result::Result<ActView, Refusal>> {
    append(ledger, anchor, actor, at, Draft::Copy { message })
}

/// Keep that a person said they pasted a copy. `refers_to` names the copy; when
/// it is `None` the latest copy on the thread is meant. Refused when the thread
/// has no copy, or the copy already carries a standing mark.
pub fn marked_pasted(
    ledger: &Ledger,
    anchor: &Anchor,
    actor: &str,
    refers_to: Option<&str>,
    at: &str,
) -> Result<std::result::Result<ActView, Refusal>> {
    append(ledger, anchor, actor, at, Draft::Mark { refers_to })
}

/// Keep that a person took a pasted mark back. `refers_to` names the mark; when
/// it is `None` the latest standing mark on the thread is meant.
pub fn unmarked_pasted(
    ledger: &Ledger,
    anchor: &Anchor,
    actor: &str,
    refers_to: Option<&str>,
    at: &str,
) -> Result<std::result::Result<ActView, Refusal>> {
    append(ledger, anchor, actor, at, Draft::Unmark { refers_to })
}

enum Draft<'a> {
    Copy { message: &'a str },
    Mark { refers_to: Option<&'a str> },
    Unmark { refers_to: Option<&'a str> },
}

fn append(
    ledger: &Ledger,
    anchor: &Anchor,
    actor: &str,
    at: &str,
    draft: Draft<'_>,
) -> Result<std::result::Result<ActView, Refusal>> {
    let actor = actor.trim();
    if actor.is_empty() {
        return Ok(Err(Refusal::NoActor));
    }
    if let Draft::Copy { message } = &draft {
        if let Err(refusal) = check_message(message) {
            return Ok(Err(refusal));
        }
    }
    let conn = ledger.conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<std::result::Result<ActView, Refusal>> {
        let anchored = conn
            .query_row(
                "SELECT 1 FROM events WHERE id=?1",
                params![anchor.thread_event_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !anchored {
            return Ok(Err(Refusal::NoSuchThread));
        }
        let earlier = stored_for(ledger, &anchor.thread_id)?;
        let (act, refers_to, message) = match draft {
            Draft::Copy { message } => (Act::Copied, None, Some(message)),
            Draft::Mark { refers_to } => {
                let copy = match refers_to {
                    Some(given) => earlier
                        .iter()
                        .find(|row| row.id == given && row.act == Act::Copied),
                    None => earlier.iter().rev().find(|row| row.act == Act::Copied),
                };
                let Some(copy) = copy else {
                    return Ok(Err(Refusal::NoCopyToMark));
                };
                if let Some(mark) = standing_marks(&earlier)
                    .into_iter()
                    .find(|mark| mark.refers_to.as_deref() == Some(copy.id.as_str()))
                {
                    return Ok(Err(Refusal::AlreadyMarked {
                        mark: mark.id.clone(),
                    }));
                }
                (Act::MarkedPasted, Some(copy.id.clone()), None)
            }
            Draft::Unmark { refers_to } => {
                let marks = standing_marks(&earlier);
                let mark = match refers_to {
                    Some(given) => marks.into_iter().find(|mark| mark.id == given),
                    None => marks.into_iter().next_back(),
                };
                let Some(mark) = mark else {
                    return Ok(Err(Refusal::NoMarkToUndo));
                };
                (Act::UnmarkedPasted, Some(mark.id.clone()), None)
            }
        };
        let id = mint();
        let digest = message.map(|text| sha256_hex(text.as_bytes()));
        conn.execute(
            "INSERT INTO change_agent_messages(id, act, thread_id, thread_event_id, change_id,
                                               version_id, message, message_sha256, actor,
                                               recorded_at, refers_to)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                id,
                act.as_str(),
                anchor.thread_id,
                anchor.thread_event_id,
                anchor.change_id,
                anchor.version_id,
                message,
                digest,
                actor,
                at,
                refers_to,
            ],
        )?;
        let mut record = MaskRecord::default();
        Ok(Ok(view(
            ViewParts {
                id,
                act,
                anchor,
                actor,
                at,
                refers_to,
                message,
                digest,
            },
            &mut record,
        )))
    })();
    finish(conn, outcome)
}

struct ViewParts<'a> {
    id: String,
    act: Act,
    anchor: &'a Anchor,
    actor: &'a str,
    at: &'a str,
    refers_to: Option<String>,
    message: Option<&'a str>,
    digest: Option<String>,
}

fn view(parts: ViewParts<'_>, record: &mut MaskRecord) -> ActView {
    ActView {
        id: parts.id,
        act: parts.act,
        thread_id: parts.anchor.thread_id.clone(),
        thread_event_id: parts.anchor.thread_event_id.clone(),
        change_id: parts.anchor.change_id.clone(),
        version_id: parts.anchor.version_id.clone(),
        actor: crate::change::mask_text(parts.actor, record),
        recorded_at: parts.at.to_string(),
        refers_to: parts.refers_to,
        message: parts
            .message
            .map(|text| crate::change::mask_text(text, record)),
        message_sha256: parts.digest,
    }
}

/// Every act on one thread, oldest first, enough to decide what a new act may
/// refer to.
fn stored_for(ledger: &Ledger, thread_id: &str) -> Result<Vec<Stored>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, act, refers_to FROM change_agent_messages
          WHERE thread_id=?1 ORDER BY recorded_at, rowid",
    )?;
    let mapped = statement.query_map(params![thread_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in mapped {
        let (id, act, refers_to) = row?;
        out.push(Stored {
            id,
            act: Act::parse_stored(&act)?,
            refers_to,
        });
    }
    Ok(out)
}

/// The pasted marks no later undo took back, oldest first.
fn standing_marks(rows: &[Stored]) -> Vec<&Stored> {
    rows.iter()
        .filter(|row| row.act == Act::MarkedPasted)
        .filter(|mark| {
            !rows.iter().any(|undo| {
                undo.act == Act::UnmarkedPasted && undo.refers_to.as_deref() == Some(&mark.id)
            })
        })
        .collect()
}

/// Every act on one thread, oldest first, masked. What the masker did is merged
/// into `record`.
pub fn read(ledger: &Ledger, thread_id: &str, record: &mut MaskRecord) -> Result<Vec<ActView>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, act, thread_event_id, change_id, version_id, message, message_sha256,
                actor, recorded_at, refers_to
           FROM change_agent_messages WHERE thread_id=?1 ORDER BY recorded_at, rowid",
    )?;
    type Raw = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
    );
    let mapped = statement.query_map(params![thread_id], |row| {
        Ok::<Raw, rusqlite::Error>((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in mapped {
        let (
            id,
            act,
            thread_event_id,
            change_id,
            version_id,
            message,
            message_sha256,
            actor,
            recorded_at,
            refers_to,
        ) = row?;
        let anchor = Anchor {
            thread_id: thread_id.to_string(),
            thread_event_id,
            change_id,
            version_id,
        };
        out.push(view(
            ViewParts {
                id,
                act: Act::parse_stored(&act)?,
                anchor: &anchor,
                actor: &actor,
                at: &recorded_at,
                refers_to,
                message: message.as_deref(),
                digest: message_sha256,
            },
            record,
        ));
    }
    Ok(out)
}

/// The last act on every thread that has one, keyed by thread id. One scan of a
/// table a human fills by hand; the order is the order the rows were appended,
/// so the last row of a thread is its last act.
pub fn last_acts(ledger: &Ledger) -> Result<BTreeMap<String, ToldAct>> {
    let mut statement = ledger.conn().prepare(
        "SELECT thread_id, act, recorded_at, version_id, id
           FROM change_agent_messages ORDER BY recorded_at, rowid",
    )?;
    let mapped = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut out = BTreeMap::new();
    for row in mapped {
        let (thread_id, act, at, version_id, act_id) = row?;
        out.insert(
            thread_id,
            ToldAct {
                last_act: Act::parse_stored(&act)?,
                at,
                version_id,
                act_id,
            },
        );
    }
    Ok(out)
}

/// The Change that tracks a thread, if one does: the Change in the thread's
/// recording project whose request is the thread's first work prompt — the
/// same match "Track this work as a Change" uses to answer `already_tracked`.
pub fn tracking_change(
    ledger: &Ledger,
    ledger_project_id: &str,
    first_prompt_event_id: &str,
) -> Result<Option<String>> {
    Ok(ledger
        .conn()
        .query_row(
            "SELECT id FROM changes WHERE project_id=?1 AND request_event_id=?2
              ORDER BY opened_at, id LIMIT 1",
            params![ledger_project_id, first_prompt_event_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

/// Commit what the closure appended, or roll it back with the error.
fn finish<T>(conn: &rusqlite::Connection, outcome: Result<T>) -> Result<T> {
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
