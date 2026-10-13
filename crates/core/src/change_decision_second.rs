//! C30 R-LEDGER — a second person approving the same version.
//!
//! `change_decision::approve` refuses a second approval of the standing version
//! on purpose: one approval stands at a time, and a second `change_decisions`
//! row would make the SECOND stand, stamp `superseded_by` on the first and
//! replace the approval's decision file. Every existing reading would change
//! meaning. So a second person's approval is its own kind of row, in
//! `change_decision_seconds`, joined to the approval it adds to (C34 ruling Q1).
//!
//! WHAT A SECOND IS NOT. It never makes an approval stand, never changes which
//! approval stands, never moves readiness, coverage or approval availability,
//! and never rewrites the decision file (which stays schema 3, ruling Q11). It
//! stops counting the moment its approval is withdrawn or replaced, which is
//! derived at read, never stored. It is free (ruling Q2): the paid act is
//! approving a version, and nothing here asks `licence_cmd::entitled`.
//!
//! WHO COUNTS AS ANOTHER PERSON. Names are compared trimmed and case-folded
//! (ruling Q3). ThruLine records the name that was typed; it does not check who
//! is at the keyboard — and every refusal here says so ([`NAME_NOT_CHECKED`]).
//!
//! This module lives beside `change_decision`, not inside it, so the approval
//! writer stays byte-untouched by the commit that adds the table.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use ulid::Ulid;

use crate::change_decision::{self, Decide};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The table, named once for the screens that cite a row by it.
pub const TABLE: &str = "change_decision_seconds";

/// The two acts, and there is no third.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecondAct {
    Approved,
    Withdrawn,
}

impl SecondAct {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Withdrawn => "withdrawn",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "approved" => Ok(Self::Approved),
            "withdrawn" => Ok(Self::Withdrawn),
            other => Err(CoreError::Integrity(format!(
                "a second approval act must be approved or withdrawn, not {other:?}"
            ))),
        }
    }
}

/// One row, exactly as the table holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecondRow {
    pub id: String,
    pub act: SecondAct,
    /// The approval this joins.
    pub decision_id: String,
    /// COPIED from the approval, never inherited (B6.13).
    pub target_revision: String,
    pub scope_version: u32,
    pub actor: String,
    pub recorded_at: String,
    /// On a withdrawal, the second it takes back.
    pub withdraws: Option<String>,
}

/// What came of one [`second`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Seconding {
    Recorded {
        second: SecondRow,
    },
    /// No approval stands on this Change, so there is nothing to second.
    NothingToSecond,
    /// The screen was showing a version other than the one the standing
    /// approval is about (`approve`'s shape). `changed` is empty when the
    /// record cannot name what moved.
    Moved {
        seen: String,
        now: String,
        changed: Vec<String>,
    },
    /// The person is the approval's own actor, or a person who already seconded
    /// it. `with` is that earlier name as the record holds it.
    SamePerson {
        with: String,
    },
}

/// What came of one [`withdraw`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Withdrawing {
    Recorded {
        withdrawal: SecondRow,
    },
    /// The person has no second standing on the approval that stands.
    NoSecondToWithdraw,
}

/// Names compared trimmed and case-folded.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/* ── the words a person reads: one place, so the terminal, the route and the
 * screen say the same thing ─────────────────────────────────────────────── */

/// Ruling Q3, verbatim. Said before a second is made and again when a name is
/// refused, because the refusal can only compare names: it cannot know who is
/// at the keyboard.
pub const NAME_NOT_CHECKED: &str =
    "ThruLine records the name you type; it does not check who is at the keyboard.";

/// No approval stands, so there is nothing to second.
pub const NOTHING_TO_SECOND: &str = "No version is approved on this Change right now, so there \
     is nothing to second. Nothing was saved.";

/// The version the screen was showing is not the one that is approved.
pub const MOVED: &str = "The approved version is not the one this screen was showing, so \
     nothing was recorded. Reload to see the version that is approved.";

/// The refusal for a name that is already on the approved version. `with` is
/// the earlier name as the record holds it.
pub fn same_person_sentence(with: &str) -> String {
    format!(
        "A second approval has to come from a different name than {}, who is already on this \
         version. {NAME_NOT_CHECKED}",
        with.trim()
    )
}

/// The refusal for taking back a second that is not standing.
pub fn no_second_to_withdraw_sentence(actor: &str) -> String {
    format!(
        "{} has no second approval standing on the approved version, so there is nothing to \
         take back. Nothing was saved.",
        actor.trim()
    )
}

/// A name or path as it is served: masked by the Change layer's one masking
/// helper (already classified in `redact::MASK_CALL_SITES`), so this module adds
/// no masking call site of its own.
pub fn masked_text(text: &str) -> String {
    let mut record = MaskRecord::default();
    crate::change::mask_text(text, &mut record)
}

/// One row as it is served on the Change: masked, with `counting` derived at
/// read. A second is a name and a time, never a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecondView {
    pub id: String,
    pub act: SecondAct,
    /// The approval this joins.
    pub decision_id: String,
    pub target_revision: String,
    pub scope_version: u32,
    pub actor: String,
    pub recorded_at: String,
    /// On a withdrawal, the second it takes back; `null` on an approval.
    pub withdraws: Option<String>,
    /// DERIVED, never stored: true only for a second that no later withdrawal
    /// took back AND whose approval is the one that stands now.
    pub counting: bool,
}

impl SecondRow {
    /// This row as a route answers for the act that just wrote it: masked on
    /// its own, with no Change-wide treatment to merge into.
    pub fn served(&self, counting: bool) -> SecondView {
        self.view(counting, &mut MaskRecord::default())
    }

    /// This row as it is served: masked. `counting` is the caller's reading.
    pub fn view(&self, counting: bool, record: &mut MaskRecord) -> SecondView {
        SecondView {
            id: self.id.clone(),
            act: self.act,
            decision_id: self.decision_id.clone(),
            target_revision: self.target_revision.clone(),
            scope_version: self.scope_version,
            actor: crate::change::mask_text(&self.actor, record),
            recorded_at: self.recorded_at.clone(),
            withdraws: self.withdraws.clone(),
            counting,
        }
    }
}

/// Whether this store holds a Change by that id.
pub fn exists(ledger: &Ledger, change_id: &str) -> Result<bool> {
    Ok(ledger
        .conn()
        .query_row(
            "SELECT 1 FROM changes WHERE id=?1",
            params![change_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Every second recorded on this approval, in the order they were appended.
pub fn seconds_for(ledger: &Ledger, decision_id: &str) -> Result<Vec<SecondRow>> {
    collect(
        ledger,
        "SELECT id, act, decision_id, target_revision, scope_version, actor,
                recorded_at, withdraws
           FROM change_decision_seconds WHERE decision_id=?1 ORDER BY recorded_at, rowid",
        decision_id,
    )
}

/// Every second row on this Change, whichever approval it joined, in the order
/// they were appended.
fn seconds_of_change(ledger: &Ledger, change_id: &str) -> Result<Vec<SecondRow>> {
    collect(
        ledger,
        "SELECT id, act, decision_id, target_revision, scope_version, actor,
                recorded_at, withdraws
           FROM change_decision_seconds WHERE change_id=?1 ORDER BY recorded_at, rowid",
        change_id,
    )
}

fn collect(ledger: &Ledger, sql: &str, key: &str) -> Result<Vec<SecondRow>> {
    let mut statement = ledger.conn().prepare(sql)?;
    let mapped = statement.query_map(params![key], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in mapped {
        let (id, act, decision_id, target_revision, scope_version, actor, recorded_at, withdraws) =
            row?;
        out.push(SecondRow {
            id,
            act: SecondAct::parse(&act)?,
            decision_id,
            target_revision,
            scope_version: scope_version.max(0) as u32,
            actor,
            recorded_at,
            withdraws,
        });
    }
    Ok(out)
}

/// The seconds still counting: approvals that no later withdrawal took back.
pub fn standing(rows: &[SecondRow]) -> Vec<&SecondRow> {
    rows.iter()
        .filter(|r| r.act == SecondAct::Approved)
        .filter(|r| {
            !rows
                .iter()
                .any(|w| w.act == SecondAct::Withdrawn && w.withdraws.as_deref() == Some(&r.id))
        })
        .collect()
}

/// How many seconds still count on one approval.
pub fn standing_count(ledger: &Ledger, decision_id: &str) -> Result<u32> {
    let rows = seconds_for(ledger, decision_id)?;
    Ok(standing(&rows).len() as u32)
}

/// Every second row on one Change, oldest first, masked. `standing_approval` is
/// the id of the approval that stands now, if one does: a second counts only on
/// that approval and only if no later withdrawal took it back. What the masker
/// did is merged into `record`, so the Change's own `masking` treatment accounts
/// for these names too.
pub fn read(
    ledger: &Ledger,
    change_id: &str,
    standing_approval: Option<&str>,
    record: &mut MaskRecord,
) -> Result<Vec<SecondView>> {
    let rows = seconds_of_change(ledger, change_id)?;
    let still: Vec<String> = standing(&rows).iter().map(|r| r.id.clone()).collect();
    Ok(rows
        .iter()
        .map(|row| {
            let counting = row.act == SecondAct::Approved
                && standing_approval == Some(row.decision_id.as_str())
                && still.contains(&row.id);
            row.view(counting, record)
        })
        .collect())
}

/// A second person approves the version the standing approval is about.
///
/// Refused three ways, none of which appends anything: nothing stands
/// ([`Seconding::NothingToSecond`]), the screen's version is not the approval's
/// ([`Seconding::Moved`]), or the person is the approval's actor or one who
/// already seconded it ([`Seconding::SamePerson`]). The standing approval is read
/// again inside the append's IMMEDIATE transaction, so two seconds from the same
/// name racing append one between them.
pub fn second(ledger: &Ledger, change_id: &str, decide: &Decide) -> Result<Seconding> {
    let actor = decide.actor.trim();
    if actor.is_empty() {
        return Err(CoreError::Config(
            "a second approval must name who made it".to_string(),
        ));
    }
    let Some(approval) = change_decision::read(ledger, change_id)?.standing else {
        return Ok(Seconding::NothingToSecond);
    };
    /* The moved-tree comparison shells out to git, so it happens before the
     * write lock is taken, exactly as `approve` and `withdraw` do. */
    if decide.seen_revision != approval.target_revision {
        return Ok(Seconding::Moved {
            changed: change_decision::changed_files(
                ledger,
                change_id,
                &decide.seen_revision,
                &approval.target_revision,
            )?,
            seen: decide.seen_revision.clone(),
            now: approval.target_revision,
        });
    }
    let conn = ledger.conn();
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<Seconding> {
        let Some(approval_now) = change_decision::read(ledger, change_id)?.standing else {
            return Ok(Seconding::NothingToSecond);
        };
        if approval_now.id != approval.id {
            return Ok(Seconding::Moved {
                seen: decide.seen_revision.clone(),
                now: approval_now.target_revision,
                changed: Vec::new(),
            });
        }
        if same_name(&approval_now.actor, actor) {
            return Ok(Seconding::SamePerson {
                with: approval_now.actor,
            });
        }
        let earlier = seconds_for(ledger, &approval_now.id)?;
        if let Some(already) = standing(&earlier)
            .into_iter()
            .find(|s| same_name(&s.actor, actor))
        {
            return Ok(Seconding::SamePerson {
                with: already.actor.clone(),
            });
        }
        let row = SecondRow {
            id: format!("cds_{}", Ulid::generate()),
            act: SecondAct::Approved,
            decision_id: approval_now.id.clone(),
            target_revision: approval_now.target_revision.clone(),
            scope_version: approval_now.scope_version,
            actor: actor.to_string(),
            recorded_at: decide.at.clone(),
            withdraws: None,
        };
        insert(ledger, change_id, &row)?;
        Ok(Seconding::Recorded { second: row })
    })();
    finish(conn, outcome)
}

/// A person takes back their own standing second on the approval that stands.
///
/// Refused one way, appending nothing: that person has no second standing
/// ([`Withdrawing::NoSecondToWithdraw`]) — including when no approval stands, or
/// when the second was on an approval since replaced. Names are compared as
/// [`same_name`] does. The row copies the approval's version and criteria
/// version, points at the second it takes back, and changes nothing else.
pub fn withdraw(ledger: &Ledger, change_id: &str, actor: &str, at: &str) -> Result<Withdrawing> {
    let actor = actor.trim();
    if actor.is_empty() {
        return Err(CoreError::Config(
            "taking back a second approval must name who is doing it".to_string(),
        ));
    }
    let conn = ledger.conn();
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<Withdrawing> {
        let Some(approval) = change_decision::read(ledger, change_id)?.standing else {
            return Ok(Withdrawing::NoSecondToWithdraw);
        };
        let earlier = seconds_for(ledger, &approval.id)?;
        let Some(theirs) = standing(&earlier)
            .into_iter()
            .find(|s| same_name(&s.actor, actor))
        else {
            return Ok(Withdrawing::NoSecondToWithdraw);
        };
        let row = SecondRow {
            id: format!("cds_{}", Ulid::generate()),
            act: SecondAct::Withdrawn,
            decision_id: approval.id.clone(),
            target_revision: approval.target_revision.clone(),
            scope_version: approval.scope_version,
            actor: actor.to_string(),
            recorded_at: at.to_string(),
            withdraws: Some(theirs.id.clone()),
        };
        insert(ledger, change_id, &row)?;
        Ok(Withdrawing::Recorded { withdrawal: row })
    })();
    finish(conn, outcome)
}

fn insert(ledger: &Ledger, change_id: &str, row: &SecondRow) -> Result<()> {
    ledger.conn().execute(
        "INSERT INTO change_decision_seconds(id, change_id, act, decision_id,
                                             target_revision, scope_version,
                                             actor, recorded_at, withdraws)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            row.id,
            change_id,
            row.act.as_str(),
            row.decision_id,
            row.target_revision,
            row.scope_version,
            row.actor,
            row.recorded_at,
            row.withdraws,
        ],
    )?;
    Ok(())
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
