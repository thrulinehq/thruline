//! A person's Inbox hide, kept until that row has another recorded event.
//!
//! The address is derived by the HQ index from the ordered event IDs of one
//! thread and the row kind. It contains no text or clock: a new recorded step
//! changes the digest, including when it is inserted before the last event.
//! The client must send back the digest it read. The writer checks the index
//! fingerprint inside its write transaction before appending an act, so a
//! click on an old row cannot silently hide its newer form.

use std::collections::BTreeMap;

use rusqlite::params;
use serde::Serialize;
use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::hq::{self, Index};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Thread,
    Prompt,
}

impl RowKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "thread" => Some(Self::Thread),
            "prompt" => Some(Self::Prompt),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thread => "thread",
            Self::Prompt => "prompt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Act {
    Hidden,
    Shown,
}

impl Act {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "hidden" => Some(Self::Hidden),
            "shown" => Some(Self::Shown),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Shown => "shown",
        }
    }
}

/// The address of a current Inbox row, chosen by the server's HQ index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Address {
    pub row_kind: RowKind,
    pub thread_id: String,
    pub thread_event_id: String,
    pub row_sha256: String,
}

/// One thread's full ordered event identity, computed once as the HQ index is
/// built. Fixed-width lengths distinguish otherwise identical concatenations.
pub fn event_ids_sha256<'a>(event_ids: impl IntoIterator<Item = &'a str>) -> String {
    let mut hash = Sha256::new();
    hash.update(b"inbox-events-v1\0");
    for id in event_ids {
        hash.update((id.len() as u64).to_be_bytes());
        hash.update(id.as_bytes());
    }
    crate::chain::to_hex(&hash.finalize())
}

/// The served address for one kind of that thread, cheap to derive from the
/// precomputed event identity. Both kinds have distinct address domains.
pub fn row_sha256_from_events(kind: RowKind, events_sha256: &str) -> String {
    let mut bytes = b"inbox-row-v1\0".to_vec();
    bytes.extend_from_slice(kind.as_str().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(events_sha256.as_bytes());
    sha256_hex(&bytes)
}

/// The same address from raw IDs, useful where no HQ index is held.
pub fn row_sha256<'a>(kind: RowKind, event_ids: impl IntoIterator<Item = &'a str>) -> String {
    row_sha256_from_events(kind, &event_ids_sha256(event_ids))
}

/// The last appended act on a kind of row, regardless of its old digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastAct {
    pub id: String,
    pub act: Act,
    pub row_sha256: String,
    pub actor: String,
    pub recorded_at: String,
}

pub type LastActs = BTreeMap<(String, RowKind), LastAct>;

/// One scan of the small, human-authored table. Rowid is append order, even
/// when two acts share a timestamp.
pub fn last_acts(ledger: &Ledger) -> Result<LastActs> {
    let mut stmt = ledger.conn().prepare(
        "SELECT id, act, row_kind, thread_id, row_sha256, actor, recorded_at
           FROM inbox_hides ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (id, act, kind, thread_id, row_sha256, actor, recorded_at) = row?;
        let act = Act::parse(&act)
            .ok_or_else(|| CoreError::Integrity("invalid inbox hide act".into()))?;
        let kind = RowKind::parse(&kind)
            .ok_or_else(|| CoreError::Integrity("invalid inbox hide row kind".into()))?;
        out.insert(
            (thread_id, kind),
            LastAct {
                id,
                act,
                row_sha256,
                actor,
                recorded_at,
            },
        );
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowState {
    #[serde(flatten)]
    pub address: Address,
    pub hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub act_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<String>,
}

pub fn state_for(address: Address, acts: &LastActs) -> RowState {
    let last = acts
        .get(&(address.thread_id.clone(), address.row_kind))
        .filter(|act| act.row_sha256 == address.row_sha256);
    let mut mask = MaskRecord::default();
    RowState {
        hidden: last.is_some_and(|act| act.act == Act::Hidden),
        act_id: last.map(|act| act.id.clone()),
        actor: last.map(|act| crate::change::mask_text(&act.actor, &mut mask)),
        recorded_at: last.map(|act| act.recorded_at.clone()),
        address,
    }
}

pub fn current_rows(index: &Index, now_ms: i64, acts: &LastActs) -> Vec<RowState> {
    index
        .inbox_addresses(now_ms)
        .into_iter()
        .map(|at| state_for(at, acts))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NoSuchRow,
    Changed,
    NoActor,
    AlreadyHidden,
    NotHidden,
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoSuchRow => "no_such_row",
            Self::Changed => "row_changed",
            Self::NoActor => "actor_required",
            Self::AlreadyHidden => "already_hidden",
            Self::NotHidden => "not_hidden",
        }
    }

    pub fn plain(&self) -> &'static str {
        match self {
            Self::NoSuchRow => "That Inbox row is no longer in this project. Reload the page.",
            Self::Changed => {
                "The record changed since this Inbox row was read. Reload it before hiding the row."
            }
            Self::NoActor => "Hiding an Inbox row needs a name on the record.",
            Self::AlreadyHidden => "That Inbox row is already hidden.",
            Self::NotHidden => "That Inbox row is not hidden now.",
        }
    }
}

/// Inputs for a hide or show act. `index_fp` is the fingerprint the route
/// built its index from; the writer checks it after taking SQLite's write lock.
pub struct KeepRequest<'a> {
    pub index_fp: &'a str,
    pub thread_id: &'a str,
    pub kind: RowKind,
    pub expected_sha256: &'a str,
    pub now_ms: i64,
    pub act: Act,
    pub actor: &'a str,
    pub at: &'a str,
}

/// Append a hide or show act after checking the request against the current row.
pub fn keep(
    ledger: &Ledger,
    index: &Index,
    request: KeepRequest<'_>,
) -> Result<std::result::Result<RowState, Refusal>> {
    let KeepRequest {
        index_fp,
        thread_id,
        kind,
        expected_sha256,
        now_ms,
        act,
        actor,
        at,
    } = request;
    let actor = actor.trim();
    if actor.is_empty() {
        return Ok(Err(Refusal::NoActor));
    }
    let conn = ledger.conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<std::result::Result<RowState, Refusal>> {
        if hq::fingerprint(ledger, index.project())? != index_fp {
            return Ok(Err(Refusal::Changed));
        }
        let Some(address) = index.inbox_address(thread_id, kind, now_ms) else {
            return Ok(Err(Refusal::NoSuchRow));
        };
        if address.row_sha256 != expected_sha256 {
            return Ok(Err(Refusal::Changed));
        }
        let acts = last_acts(ledger)?;
        let state = state_for(address.clone(), &acts);
        if act == Act::Hidden && state.hidden {
            return Ok(Err(Refusal::AlreadyHidden));
        }
        if act == Act::Shown && !state.hidden {
            return Ok(Err(Refusal::NotHidden));
        }
        let id = format!("ibh_{}", Ulid::generate());
        let shows = if act == Act::Shown {
            state.act_id.as_deref()
        } else {
            None
        };
        conn.execute(
            "INSERT INTO inbox_hides(id, act, row_kind, thread_id, thread_event_id,
                                     row_sha256, actor, recorded_at, shows)
              VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                id,
                act.as_str(),
                kind.as_str(),
                address.thread_id,
                address.thread_event_id,
                address.row_sha256,
                actor,
                at,
                shows
            ],
        )?;
        Ok(Ok(RowState {
            address,
            hidden: act == Act::Hidden,
            act_id: Some(id),
            actor: Some(crate::change::mask_text(actor, &mut MaskRecord::default())),
            recorded_at: Some(at.to_string()),
        }))
    })();
    match outcome {
        Ok(value) => {
            conn.execute_batch("COMMIT")?;
            Ok(value)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}
