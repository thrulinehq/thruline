//! T-S-4 — the decision: approving exactly one version, and taking it back.
//!
//! Sam's question is "what exactly did I just approve, and can I take it back?"
//! The answer has to be a record rather than a screen state, so this module is
//! the whole of it: two acts, both appends, both naming everything they are
//! about.
//!
//! WHAT AN APPROVAL NAMES. Exactly one target version (the standing snapshot,
//! or the base revision when no snapshot is bound), one actor, one time, and
//! one accepted-criteria version. All four are COLUMNS of the row, never a
//! lookup at read time, because a lookup is how an approval silently inherits
//! across a version it was never given for (`B6.13`).
//!
//! WHAT IT REFUSES. If the version the screen was showing is not the version
//! the record stands at now, the approval is refused and the refusal NAMES THE
//! FILES that moved. A person who clicked Approve while an agent was writing
//! must not be told they approved the thing they were reading.
//!
//! WHAT IT DOES NOT DO. Nothing acts on the repository. No merge, no push, no
//! deploy, no message to an agent. The only side effect outside the ledger is a
//! JSON file under the project's own worktree at
//! `.thruline/decisions/<version>.json`, which is a copy of the row for a
//! person who has the folder and not the tool.
//!
//! DECISION-FILE-AFTER-WITHDRAW. That file is a DERIVED copy of the latest act
//! on its version, and says so (`"derived_copy": true`, the row's own id). A
//! withdrawal replaces it with a copy of the withdrawal row; a later approval
//! of the same version replaces it with a copy of that approval. Each
//! replacement is a whole new file renamed over the old one in the same
//! folder, made visible only after the row is in the ledger and before that
//! row is committed — so a reader never sees "approved" for an approval that
//! no longer stands, and never sees half a file. The ledger rows themselves
//! are never edited.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use ulid::Ulid;

use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// The shape of `.thruline/decisions/<version>.json`. Bumped when a field is
/// added, so a reader of an old file is never guessing. **2** since T-5-3:
/// `exceptions`, and an `act` that may read `approved_with_exceptions`.
/// **3** since DECISION-FILE-AFTER-WITHDRAW: `derived_copy` (always `true`),
/// `decision_id` (the row this file copies) and, on a withdrawal's copy only,
/// `withdraws` (the approval it takes back); and `act` may now read
/// `withdrawn`, because the file follows the latest act on its version.
pub const DECISION_RECORD_SCHEMA_VERSION: u32 = 3;

/// The shape every approval copy was written in before schema 3. An untouched
/// file written by an older release is still compared with what that release
/// wrote for its row ([`record_document_v2_for_row`]), so upgrading never
/// turns an intact copy into a "changed" one.
pub const DECISION_RECORD_SCHEMA_VERSION_2: u32 = 2;

/// One thing an approval was given **under** rather than **on** (`B7.20`,
/// `B10.9`).
///
/// Every field is COPIED onto the approval's own row rather than joined to the
/// waiver at read time, for the same reason the version is: an approval
/// that looked its exceptions up would silently acquire one written after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionException {
    /// The appended waiver this was taken from, for a reader who wants
    /// the original record.
    pub waiver_id: String,
    pub obligation_id: String,
    /// `waived` or `not_applicable`.
    pub kind: String,
    pub actor: String,
    /// The accepted policy's authority the exception rests on (`B7.19`).
    pub authority: String,
    pub rationale: String,
}

/// The folder, under the project's own worktree, the records are written to.
pub const DECISIONS_DIR: &str = ".thruline/decisions";

/// The two acts, and there is no third. In particular there is no `amended`:
/// a second thought is a withdrawal plus a new approval, both visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAct {
    Approved,
    Withdrawn,
}

impl DecisionAct {
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
            other => Err(CoreError::Config(format!(
                "a decision act must be approved or withdrawn, not {other}"
            ))),
        }
    }
}

/// One recorded act, exactly as the row holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionRow {
    pub id: String,
    pub act: DecisionAct,
    /// The ONE version this act is about. Short forms are the screen's job.
    pub target_revision: String,
    /// The accepted-criteria version this act is about.
    pub scope_version: u32,
    pub actor: String,
    pub decided_at: String,
    /// The approval a withdrawal withdraws; `None` on an approval.
    pub withdraws: Option<String>,
    /// Where the JSON copy was written, when it could be.
    pub record_path: Option<String>,
    /// **`B7.20`, `B7.22`, `B10.9`.** What this act was given under: every
    /// obligation that cleared on a person's word rather than on evidence,
    /// with who, under what authority and why. Empty on an ordinary approval
    /// and on every withdrawal. A surface reads THIS, not the live
    /// waivers, so an approval can never acquire an exception it was not
    /// given.
    ///
    /// **In the ONE order `in_canonical_order` defines**, whichever way the
    /// row came to be (just recorded, or read back later) and whichever
    /// surface serves it, so the same approval never lists its exceptions two
    /// ways (DECISION-HISTORY-SHAPE-R4).
    #[serde(default)]
    pub exceptions: Vec<DecisionException>,
    /// **T-5-2-NO-REVIVAL (R-2026-09-28-10).** The later approval that
    /// replaced this one, when one did and this one was not withdrawn first.
    /// Derived at read time from the order the acts were appended, never
    /// stored, and `None` on every withdrawal and on every act as it is
    /// recorded. A surface prints it so an approval that stopped standing
    /// because a newer one arrived is never mistaken for one still in force.
    ///
    /// **DECISION-HISTORY-SHAPE (Q19 V1 E7, QD-11) — the one rule for every
    /// surface that serves a decision row** (the served Change, the
    /// `/decisions` routes, the export, the CLI JSON — all serialize THIS
    /// struct): a *stored* field is always present, `null` when empty
    /// (`withdraws`, `record_path`); a *derived* field is **always absent when
    /// it does not apply** and present only when it does. So a row serves the
    /// same bytes at every point in time until a decision act changes what it
    /// says, and a row nothing replaced serves exactly the fields it served
    /// before this field existed — the choice that leaves every existing
    /// expected value untouched (the older release's history, captured before
    /// an upgrade, still equals the history read after it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    /// **C30 R-LEDGER.** The words the person kept with this act, masked, when
    /// they kept any. Derived from `change_notes` at read time, never a column
    /// of `change_decisions`, and absent when there are none (the rule above),
    /// so an approval recorded without a note serves exactly the bytes it
    /// always served. It is words and never a result: nothing that decides
    /// which approval stands, or whether the Change is ready, reads it, and the
    /// decision file does not carry it (that file stays schema 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// **C30 R-LEDGER.** How many second people's approvals still count on this
    /// approval ([`crate::change_decision_second`]), carried on the STANDING
    /// approval only and absent at zero. Derived from `change_decision_seconds`
    /// at read time, never a column of `change_decisions`, so an approval nobody
    /// seconded serves exactly the bytes it always did. A second is a name and
    /// never a result: nothing that decides which approval stands, or whether
    /// the Change is ready, reads it, and the decision file does not carry it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds_standing: Option<u32>,
}

impl DecisionRow {
    /// **`B7.22`.** Was this approval given with something set aside? The one
    /// place the question is answered, so no surface can decide it twice.
    pub fn with_exceptions(&self) -> bool {
        self.act == DecisionAct::Approved && !self.exceptions.is_empty()
    }
}

/// What the record holds about this Change's decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionsView {
    /// The approval that still stands, if one does: the LATEST approval act,
    /// and only if it has not been withdrawn. An earlier approval never stands
    /// again once a later one exists, whatever happens to the later one — a
    /// withdrawal of the newest approval leaves the Change unapproved
    /// (R-2026-09-28-10). An approval for a version the record has since moved
    /// off STILL STANDS — it was given for that version and it says so.
    /// Whether the code has moved since is a separate field, below.
    pub standing: Option<DecisionRow>,
    /// Every act, oldest first. Nothing is ever removed from this list.
    pub history: Vec<DecisionRow>,
    /// The version the record stands at now, for the screen to compare against
    /// the standing approval's.
    pub target_revision: Option<String>,
    /// The accepted-criteria version the record stands at now.
    pub scope_version: Option<u32>,
}

impl DecisionsView {
    /// Whether the standing approval was given for a version that is no longer
    /// the one the record stands at. Derived, never stored: storing it would be
    /// a second answer that could age.
    pub fn code_changed_since(&self) -> bool {
        match (&self.standing, &self.target_revision) {
            (Some(row), Some(now)) => &row.target_revision != now,
            _ => false,
        }
    }
}

/// What one act asks for.
#[derive(Debug, Clone)]
pub struct Decide {
    pub actor: String,
    pub at: String,
    /// The version the screen was showing when the person clicked. The act is
    /// refused when it is not the version the record stands at.
    pub seen_revision: String,
    /// **C30 R-LEDGER.** Words the person keeps with this act (at most
    /// [`crate::change_note::MAX_NOTE_CHARS`] characters; nothing but
    /// whitespace is no note). When given, the act also appends one
    /// `change_notes` row naming the new decision row, inside the same
    /// transaction, so the act and its note are recorded together or not at
    /// all. Too long is refused before anything is written.
    pub note: Option<String>,
}

/// What came of one act.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Recorded {
        decision: DecisionRow,
    },
    /// The tree moved while the screen was open. `changed` is what moved,
    /// as paths; it is empty when the record cannot name them, and the caller
    /// says so rather than implying nothing moved.
    Moved {
        seen: String,
        now: String,
        changed: Vec<String>,
    },
}

/* ── reading ───────────────────────────────────────────────────────────── */

/// Every act recorded for one Change, and where the record stands now.
pub fn read(ledger: &Ledger, change_id: &str) -> Result<DecisionsView> {
    let mut history = rows(ledger, change_id)?;
    let withdrawn: Vec<String> = history.iter().filter_map(|r| r.withdraws.clone()).collect();
    /* T-5-2-NO-REVIVAL (R-2026-09-28-10). Only the latest approval act can
     * stand. Searching backwards for "any approval not withdrawn" is how
     * withdrawing the newest approval used to bring an older, already
     * replaced one back as standing. */
    let mut standing = history
        .iter()
        .rev()
        .find(|r| r.act == DecisionAct::Approved)
        .filter(|r| !withdrawn.contains(&r.id))
        .cloned();
    /* C30 R-LEDGER. The standing approval, and only it, says how many second
     * people's approvals still count on it; absent at zero (the rule on
     * `superseded_by`: a derived field is absent when it does not apply). */
    if let Some(row) = standing.as_mut() {
        let counting = crate::change_decision_second::standing_count(ledger, &row.id)?;
        row.seconds_standing = (counting > 0).then_some(counting);
    }
    /* And every earlier approval that was not withdrawn first names the next
     * approval, the one that replaced it. */
    let mut next_approval: Option<String> = None;
    for row in history.iter_mut().rev() {
        if row.act != DecisionAct::Approved {
            continue;
        }
        if !withdrawn.contains(&row.id) {
            row.superseded_by = next_approval.clone();
        }
        next_approval = Some(row.id.clone());
    }
    let (target_revision, scope_version) = standing_target(ledger, change_id)?;
    Ok(DecisionsView {
        standing,
        history,
        target_revision,
        scope_version,
    })
}

/// Every act, in the order they were APPENDED.
///
/// The tiebreak is `rowid`, not `id`. Two acts recorded in the same second are
/// possible — a withdrawal immediately followed by a new approval is exactly
/// that — and a ULID tiebreak orders them by a random component inside the
/// millisecond, so "which was the latest approval" would be decided by chance.
/// `rowid` on an append-only table is the order the rows arrived in.
fn rows(ledger: &Ledger, change_id: &str) -> Result<Vec<DecisionRow>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, act, target_revision, scope_version, actor, decided_at, withdraws, record_path
           FROM change_decisions WHERE change_id=?1 ORDER BY decided_at, rowid",
    )?;
    let mapped = statement.query_map(params![change_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;
    let mut notes = crate::change_note::by_decision(ledger, change_id)?;
    let mut out = Vec::new();
    for row in mapped {
        let (id, act, target_revision, scope_version, actor, decided_at, withdraws, record_path) =
            row?;
        let id_for_exceptions = id.clone();
        let note = notes.remove(&id);
        out.push(DecisionRow {
            id,
            act: DecisionAct::parse(&act)?,
            target_revision,
            scope_version: scope_version.max(0) as u32,
            actor,
            decided_at,
            withdraws,
            record_path,
            exceptions: exceptions_of(ledger, &id_for_exceptions)?,
            superseded_by: None,
            note,
            seconds_standing: None,
        });
    }
    Ok(out)
}

/// The version and accepted-criteria version the record stands at now.
///
/// The snapshot wins over the base revision, exactly as `targetVersion` in
/// `change-view.js` decides it: the snapshot is the copy of the work being
/// looked at, and the base is where it started. Two rules for one fact is how
/// a screen and an engine come to disagree about what was approved.
pub fn standing_target(ledger: &Ledger, change_id: &str) -> Result<(Option<String>, Option<u32>)> {
    let implementation = ledger
        .conn()
        .query_row(
            "SELECT snapshot_id, base_revision FROM change_implementations
              WHERE change_id=?1 ORDER BY binding_version DESC LIMIT 1",
            params![change_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let revision = match implementation {
        Some((snapshot, base)) => Some(match snapshot {
            Some(id) if !id.trim().is_empty() => id,
            _ => base,
        }),
        /* T-S-4-F1. `change_implementations` is written by nothing the product
         * ships — `append_implementation` has no caller outside tests — so
         * keying a decision on it alone made Approve permanently off on the
         * flow a person can actually walk. The version a decision names is the
         * version THE PRODUCT ALREADY RECORDS: the standing `intended`
         * examined-inputs record (T-3-2), written by `change new`, first use,
         * `change prepare` and `check --for-change`. The binding wins when one
         * exists, because an explicit binding is a stronger statement; the
         * examined-inputs record is the ordinary path. The moved-tree
         * comparison reads this same function, so both halves stay keyed to one
         * record. */
        None => intended_revision(ledger, change_id)?,
    };
    let scope_version = ledger
        .conn()
        .query_row(
            "SELECT MAX(scope_version) FROM change_scope_acceptances WHERE change_id=?1",
            params![change_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten()
        .map(|v| v.max(0) as u32);
    Ok((revision, scope_version))
}

/// The version the standing `intended` examined-inputs record names, if one
/// does: its snapshot when it captured one, and otherwise the base revision.
///
/// The same order `targetVersion` in `change-view.js` uses, for the same
/// reason: the snapshot is the exact copy of the work being looked at, and the
/// base is where it started.
pub fn intended_revision(ledger: &Ledger, change_id: &str) -> Result<Option<String>> {
    let Some(inputs) = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?
    else {
        return Ok(None);
    };
    Ok(inputs
        .snapshot_id
        .value()
        .or_else(|| inputs.base_revision.value())
        .map(|v| v.to_string())
        .filter(|v| !v.trim().is_empty()))
}

/* ── the two acts ──────────────────────────────────────────────────────── */

/// Approve exactly one version.
///
/// `project_root` is where `.thruline/decisions/<version>.json` is written. It
/// is the Change's recorded worktree path when the snapshot names one, and the
/// caller may override it; `None` means no file is written and the row says so
/// by carrying no `record_path`.
pub fn approve(ledger: &Ledger, change_id: &str, decide: &Decide) -> Result<Outcome> {
    let actor = require_actor(&decide.actor)?;
    let note = require_note(decide)?;
    let (now, scope_version) = standing_target(ledger, change_id)?;
    let Some(now) = now else {
        return Err(CoreError::Config(
            "this change has no version bound, so there is nothing to approve".to_string(),
        ));
    };
    let Some(scope_version) = scope_version else {
        return Err(CoreError::Config(
            "nothing has been accepted as what counts as done, so there is nothing to approve"
                .to_string(),
        ));
    };
    if decide.seen_revision != now {
        return Ok(Outcome::Moved {
            changed: changed_files(ledger, change_id, &decide.seen_revision, &now)?,
            seen: decide.seen_revision.clone(),
            now,
        });
    }
    /* T-5-2 (`B6.14`). Everything from here is under one write lock, because
     * "is anything approved yet?" and "append this approval" being two separate
     * reads is exactly how two clicks at the same instant become two approvals.
     * Nothing slow happens inside it — the moved comparison, which shells out
     * to git, has already returned above. */
    let guard = Immediate::begin(ledger)?;
    /* The refusal is about THIS version. An approval standing on an earlier
     * version is not an approval of this one — that is the whole of `B6.13` —
     * so it does not block this act; it simply stops standing once this one is
     * recorded, and both rows say which version each was about. What is
     * refused is a SECOND approval of the same version, which is what two
     * clicks at the same instant would otherwise produce. */
    if let Some(standing) = read(ledger, change_id)?
        .standing
        .filter(|s| s.target_revision == now)
    {
        return Err(CoreError::Config(already_approved(&standing)));
    }
    crate::decisions::gate::require_approval_clear(ledger, change_id, &now, &decide.at)?;
    /* T-5-3 (`B7.20`, `B7.22`, `B10.9`). What this approval is being given
     * UNDER, read once, here, and then COPIED onto the row. It is the set of
     * obligations that cleared on a person's word rather than on an admissible
     * item, taken from the readiness computation itself so the screen's
     * "Ready for approval with exceptions" and the approval's own label can
     * never come from two different readings. */
    let exceptions = exceptions_standing(ledger, change_id)?;
    let mut row = DecisionRow {
        id: format!("cd_{}", Ulid::generate()),
        act: DecisionAct::Approved,
        target_revision: now,
        scope_version,
        actor: actor.to_string(),
        decided_at: decide.at.clone(),
        withdraws: None,
        record_path: None,
        exceptions,
        superseded_by: None,
        note: note.map(crate::change_note::masked),
        seconds_standing: None,
    };
    /* DECISION-FILE-AFTER-WITHDRAW. The copy names this row's id, so it is
     * staged once the row exists in memory, published only after the ledger
     * has accepted the row, and the act is committed only after it is
     * visible. A copy that cannot even be staged keeps the old rule — the
     * decision stands and the row carries no path. A staged copy that then
     * cannot be renamed into place refuses the act (the guard rolls back),
     * because the row would otherwise name a file that says something else —
     * after a withdrawal, the opposite. */
    let staged = match worktree_root(ledger, change_id)? {
        Some(root) => stage_copy(
            &root,
            &row.target_revision,
            &document_bytes(change_id, &row)?,
        ),
        None => None,
    };
    row.record_path = staged.as_ref().map(StagedCopy::path_string);
    insert(ledger, change_id, &row)?;
    keep_note(ledger, change_id, &row, note, decide)?;
    if let Some(copy) = staged {
        copy.publish()?;
    }
    guard.commit()?;
    Ok(Outcome::Recorded { decision: row })
}

/// Take one approval back. Its own act, its own row, pointing at the approval.
pub fn withdraw(ledger: &Ledger, change_id: &str, decide: &Decide) -> Result<Outcome> {
    let actor = require_actor(&decide.actor)?;
    let note = require_note(decide)?;
    let view = read(ledger, change_id)?;
    let Some(standing) = view.standing else {
        return Err(CoreError::Config(
            "there is no approval standing on this change to withdraw".to_string(),
        ));
    };
    /* A withdrawal is about the approval, not about the current tree: a person
     * whose agent moved the code must still be able to take back what they
     * said. The version the screen was showing is checked against the APPROVAL,
     * so the act is still about one named thing. */
    if !decide.seen_revision.is_empty() && decide.seen_revision != standing.target_revision {
        return Ok(Outcome::Moved {
            changed: changed_files(
                ledger,
                change_id,
                &decide.seen_revision,
                &standing.target_revision,
            )?,
            seen: decide.seen_revision.clone(),
            now: standing.target_revision,
        });
    }
    /* The same lock, for the same reason: two clicks on Withdraw are two
     * withdrawals of one approval, and the second has nothing to withdraw. */
    let guard = Immediate::begin(ledger)?;
    let Some(standing) = read(ledger, change_id)?.standing else {
        return Err(CoreError::Config(
            "there is no approval standing on this change to withdraw".to_string(),
        ));
    };
    let row = DecisionRow {
        id: format!("cd_{}", Ulid::generate()),
        act: DecisionAct::Withdrawn,
        target_revision: standing.target_revision.clone(),
        scope_version: standing.scope_version,
        actor: actor.to_string(),
        decided_at: decide.at.clone(),
        withdraws: Some(standing.id.clone()),
        record_path: None,
        /* A withdrawal is given under nothing: it takes an approval back and
         * says so, and the approval it points at still carries its own. */
        exceptions: Vec::new(),
        superseded_by: None,
        note: note.map(crate::change_note::masked),
        seconds_standing: None,
    };
    /* DECISION-FILE-AFTER-WITHDRAW. The approval's copy, wherever it was
     * written, is replaced by a copy of THIS row before the act is committed.
     * The withdrawal row keeps `record_path: None` — it is the approval's file
     * that is replaced, and the export still reads that file against the
     * approval's row. A copy that is already gone says nothing stale and is
     * not re-created; one that is there and cannot be replaced refuses the
     * withdrawal, naming the file, rather than leave it saying "approved". */
    let staged = match standing.record_path.as_deref() {
        Some(path) => stage_replacement(path, &row, &document_bytes(change_id, &row)?)?,
        None => None,
    };
    insert(ledger, change_id, &row)?;
    keep_note(ledger, change_id, &row, note, decide)?;
    if let Some(copy) = staged {
        copy.publish()?;
    }
    guard.commit()?;
    Ok(Outcome::Recorded { decision: row })
}

/* ── one act at a time (T-5-2, `B6.14`) ────────────────────────────────── */

/// How long a second click waits for the first one to finish before it gives
/// up. Two people pressing Approve within the same second is the case this
/// exists for, and the winner's work is one row and one small file.
const WAIT_FOR_THE_OTHER_CLICK_MS: u64 = 5_000;

/// A write that happens once or does not happen.
///
/// `BEGIN IMMEDIATE` takes SQLite's write lock at the START of the act rather
/// than at the first write, so the check "is anything approved yet?" and the
/// append that depends on it cannot be interleaved with another process's.
/// Dropped without [`Immediate::commit`] — on any `?`, on any early return —
/// it rolls back, so a refused act leaves nothing behind.
struct Immediate<'a> {
    conn: &'a rusqlite::Connection,
    committed: bool,
}

impl<'a> Immediate<'a> {
    fn begin(ledger: &'a Ledger) -> Result<Self> {
        let conn = ledger.conn();
        conn.busy_timeout(std::time::Duration::from_millis(
            WAIT_FOR_THE_OTHER_CLICK_MS,
        ))?;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        Ok(Self {
            conn,
            committed: false,
        })
    }

    fn commit(mut self) -> Result<()> {
        self.conn.execute_batch("COMMIT")?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Immediate<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
    }
}

/// What the loser of two simultaneous approvals is told: that the version IS
/// approved, by whom and when, and what to do to change that. Never "database
/// is locked", and never a silent second row.
fn already_approved(standing: &DecisionRow) -> String {
    let day = standing
        .decided_at
        .split('T')
        .next()
        .unwrap_or(standing.decided_at.as_str());
    format!(
        "{} approved this version on {}. One approval stands at a time, so withdraw that one before approving again.",
        standing.actor, day
    )
}

fn require_actor(actor: &str) -> Result<&str> {
    let trimmed = actor.trim();
    if trimmed.is_empty() {
        return Err(CoreError::Config(
            "a decision must name who made it".to_string(),
        ));
    }
    Ok(trimmed)
}

/// The words to keep with this act, or `None` when there are none. Nothing but
/// whitespace is no note; more than the limit is refused here, before the act
/// has read or written anything.
fn require_note(decide: &Decide) -> Result<Option<&str>> {
    let Some(words) = decide.note.as_deref() else {
        return Ok(None);
    };
    match crate::change_note::check(words) {
        Ok(()) => Ok(Some(words)),
        Err(crate::change_note::NoteProblem::Empty) => Ok(None),
        Err(problem) => Err(CoreError::Config(problem.plain())),
    }
}

/// Append the note kept with `row`, inside the act's own transaction, naming
/// the row just appended and stamped with the act's own time and actor.
fn keep_note(
    ledger: &Ledger,
    change_id: &str,
    row: &DecisionRow,
    note: Option<&str>,
    decide: &Decide,
) -> Result<()> {
    if let Some(words) = note {
        crate::change_note::insert_with_decision(
            ledger, change_id, &row.id, &row.actor, words, &decide.at,
        )?;
    }
    Ok(())
}

fn insert(ledger: &Ledger, change_id: &str, row: &DecisionRow) -> Result<()> {
    ledger.conn().execute(
        "INSERT INTO change_decisions(id, change_id, act, target_revision, scope_version,
                                      actor, decided_at, withdraws, record_path)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            row.id,
            change_id,
            row.act.as_str(),
            row.target_revision,
            row.scope_version,
            row.actor,
            row.decided_at,
            row.withdraws,
            row.record_path,
        ],
    )?;
    for exception in &row.exceptions {
        ledger.conn().execute(
            "INSERT INTO change_decision_exceptions(id, decision_id, waiver_id,
                                                    obligation_id, kind, actor, authority, rationale)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                format!("ce_{}", Ulid::generate()),
                row.id,
                exception.waiver_id,
                exception.obligation_id,
                exception.kind,
                exception.actor,
                exception.authority,
                exception.rationale,
            ],
        )?;
    }
    Ok(())
}

/* ── what an approval is given under (T-5-3) ───────────────────────────── */

/// The exceptions one approval would be given under, **now**.
///
/// It reads the readiness computation rather than the waiver table
/// directly, and that is the whole point: a waiver written over an
/// obligation that has since been satisfied by a real result does NOT bind —
/// `change_readiness::compute` lets the evidence win, and this function
/// inherits that rule instead of re-deciding it. A Change with no waiver
/// recorded at all costs nothing: the table is asked first, and no coverage is
/// derived when it is empty.
fn exceptions_standing(ledger: &Ledger, change_id: &str) -> Result<Vec<DecisionException>> {
    if crate::change_readiness::waivers(ledger, change_id)?.is_empty() {
        return Ok(Vec::new());
    }
    let environment = crate::change_propose::machine_environment(ledger, change_id)?;
    let coverage = crate::change_evidence::coverage(ledger, change_id, &environment)?;
    let readiness = crate::change_readiness::readiness(ledger, change_id, &coverage)?;
    let standing: Vec<DecisionException> = readiness
        .obligations
        .iter()
        .filter(|o| !o.outcome.is_evidence())
        .filter_map(|o| {
            o.waiver.as_ref().map(|d| DecisionException {
                waiver_id: d.id.clone(),
                obligation_id: o.id.clone(),
                kind: d.kind.as_str().to_string(),
                actor: d.actor.clone(),
                authority: d.authority.clone(),
                rationale: d.rationale.clone(),
            })
        })
        .collect();
    Ok(in_canonical_order(&standing))
}

/// The exceptions one recorded act was given under, off its own rows, in the
/// one order [`in_canonical_order`] defines.
///
/// The query asks for no order that means anything: an `ORDER BY` on
/// `obligation_id` is a TEXT sort, and text puts `criterion:10` before
/// `criterion:2`. `rowid` only makes the read repeatable; the order a person
/// and the export see is imposed after it, in Rust, where the number in the id
/// is a number.
fn exceptions_of(ledger: &Ledger, decision_id: &str) -> Result<Vec<DecisionException>> {
    let mut statement = ledger.conn().prepare(
        "SELECT waiver_id, obligation_id, kind, actor, authority, rationale
           FROM change_decision_exceptions WHERE decision_id=?1 ORDER BY rowid",
    )?;
    let mapped = statement.query_map(params![decision_id], |row| {
        Ok(DecisionException {
            waiver_id: row.get(0)?,
            obligation_id: row.get(1)?,
            kind: row.get(2)?,
            actor: row.get(3)?,
            authority: row.get(4)?,
            rationale: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in mapped {
        out.push(row?);
    }
    Ok(in_canonical_order(&out))
}

/// **DECISION-HISTORY-SHAPE-R4 — the ONE order an approval's exceptions are
/// listed in, defined here and nowhere else.**
///
/// Numbered criteria first, by the NUMBER in `criterion:<n>` (2 before 10, the
/// order the Change lists its lines and the order readiness computes them in);
/// then the review obligation; then anything else by name. Ties on all of that
/// fall to the rest of the exception's own fields, so two exceptions are never
/// equal-but-unordered and the result never depends on the order the caller
/// held them in.
///
/// Every place an approval's exceptions are made, read or written goes through
/// this one function: [`exceptions_standing`] (what `approve` records and
/// returns), [`exceptions_of`] (what every later read serves) and
/// [`record_document`] (what the file holds and what the export compares it
/// with). Before it, the file and the act response listed them in readiness
/// order and the served history in `ORDER BY obligation_id` text order, which
/// differ as soon as a Change has ten criteria — and the export's
/// whole-document comparison, which is right to be sensitive to order,
/// answered "changed" for a file nobody had touched.
fn in_canonical_order(exceptions: &[DecisionException]) -> Vec<DecisionException> {
    let mut ordered = exceptions.to_vec();
    ordered.sort_by(|a, b| exception_order_key(a).cmp(&exception_order_key(b)));
    ordered
}

/// The sort key behind [`in_canonical_order`]: (kind of obligation, its number,
/// then every field, so the order is total).
type ExceptionOrderKey<'a> = (
    (u8, u64),
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
);

fn exception_order_key(e: &DecisionException) -> ExceptionOrderKey<'_> {
    let numbered = e
        .obligation_id
        .strip_prefix("criterion:")
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|n| n.parse::<u64>().ok());
    let rank = match numbered {
        Some(number) => (0, number),
        None if e.obligation_id == "review" => (1, 0),
        None => (2, 0),
    };
    (
        rank,
        &e.obligation_id,
        &e.waiver_id,
        &e.kind,
        &e.actor,
        &e.authority,
        &e.rationale,
    )
}

/* ── the file under the project ────────────────────────────────────────── */

/// The worktree root the Change's standing snapshot recorded, if it recorded one.
pub fn worktree_root(ledger: &Ledger, change_id: &str) -> Result<Option<PathBuf>> {
    let snapshot_id = ledger
        .conn()
        .query_row(
            "SELECT snapshot_id FROM change_implementations
              WHERE change_id=?1 ORDER BY binding_version DESC LIMIT 1",
            params![change_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    let Some(snapshot_id) = snapshot_id else {
        /* Same fallback as the version, and for the same reason: the folder a
         * decision record is written under is the one the product recorded. */
        return Ok(crate::examined_inputs::standing(
            ledger,
            crate::examined_inputs::SubjectKind::Intended,
            change_id,
        )?
        .and_then(|i| i.worktree_path.value().map(PathBuf::from)));
    };
    let root = ledger
        .conn()
        .query_row(
            "SELECT root FROM workspace_snapshots WHERE id=?1",
            params![snapshot_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(root.filter(|r| !r.trim().is_empty()).map(PathBuf::from))
}

/// The bytes of the copy for one row: [`record_document_for_row`], pretty.
fn document_bytes(change_id: &str, row: &DecisionRow) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(&record_document_for_row(change_id, row))
        .map_err(|e| CoreError::Config(format!("writing a decision record: {e}")))
}

/// A complete copy, written to a temporary file in the decisions folder
/// itself and not yet visible under its name. [`StagedCopy::publish`] renames
/// it over `<version>.json` — one rename in one folder, so a reader sees the
/// old file or the new one and never part of either. Dropped unpublished, the
/// temporary file is removed.
struct StagedCopy {
    file: PathBuf,
    temp: tempfile::NamedTempFile,
}

impl StagedCopy {
    fn path_string(&self) -> String {
        self.file.to_string_lossy().to_string()
    }

    fn publish(self) -> Result<()> {
        let file = self.file;
        self.temp
            .persist(&file)
            .map(|_| ())
            .map_err(|e| CoreError::io(&file, e.error))
    }
}

/// Stage the copy a person can read with the folder and without the tool,
/// under `root`.
///
/// A root that does not exist is not an error: the record is in the ledger
/// either way, and the row simply carries no path. Failing the approval because
/// a folder has moved would lose the decision to a filesystem detail. Nothing is
/// written anywhere but `root/.thruline/decisions/`: a `.thruline`, a
/// `decisions` folder or a `<version>.json` that is a symlink (or not the kind
/// of thing its name says) is not followed, and nothing is staged.
fn stage_copy(root: &Path, revision: &str, body: &[u8]) -> Option<StagedCopy> {
    if !root.is_dir() {
        return None;
    }
    let dir = root.join(DECISIONS_DIR);
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    for folder in [root.join(".thruline"), dir.clone()] {
        match std::fs::symlink_metadata(&folder) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            _ => return None,
        }
    }
    let file = record_path_for(root, revision);
    match std::fs::symlink_metadata(&file) {
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => return None,
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return None,
    }
    let mut temp = tempfile::Builder::new()
        .prefix(".decision-")
        .suffix(".tmp")
        .tempfile_in(&dir)
        .ok()?;
    use std::io::Write as _;
    temp.write_all(body).ok()?;
    temp.as_file().sync_all().ok()?;
    /* `tempfile` creates owner-only files; the copy has always been an
     * ordinary file a person or a CI job under another account can read. */
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o644))
            .ok()?;
    }
    Some(StagedCopy { file, temp })
}

/// Stage the replacement of the copy an approval recorded at `record_path`
/// with `body`, the copy of `row`.
///
/// The path is the one on the approval's own row — the folder it was actually
/// written in, even if the Change now names another — and it is used only when
/// it is exactly where [`record_path_for`] puts that version's copy, so a
/// stored path can never steer a write anywhere else. A copy that is no longer
/// there says nothing stale, so nothing is created (`Ok(None)`). A copy that
/// is there and cannot be replaced is an error that names it: the caller must
/// not record an act the file would contradict.
fn stage_replacement(
    record_path: &str,
    row: &DecisionRow,
    body: &[u8],
) -> Result<Option<StagedCopy>> {
    let path = Path::new(record_path);
    let root = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .filter(|root| record_path_for(root, &row.target_revision) == path);
    let Some(root) = root else {
        return Err(CoreError::Config(format!(
            "the approval's decision copy is not where ThruLine writes one, so it was not replaced: {record_path}"
        )));
    };
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CoreError::io(path, e)),
        Ok(_) => {}
    }
    stage_copy(root, &row.target_revision, body).map(Some).ok_or_else(|| {
        CoreError::Config(format!(
            "the decision copy at {record_path} could not be replaced, so it would still say the approval stands; nothing was recorded"
        ))
    })
}

/// **DECISION-HISTORY-SHAPE-R3.** The act's content, as data: the ONE builder
/// behind every decision record document — [`record_document_for_row`] adds
/// which row it copies, and that is what is serialized to the file and what
/// every reader compares a file with (`change_export::decision_copy_standing`).
/// A field added here is written AND compared, with no second list to keep in
/// step.
///
/// **DECISION-HISTORY-SHAPE-R4:** `exceptions` is listed in the one order
/// `in_canonical_order` defines, whatever order the caller holds them in, so
/// the file that is written and the document the export builds from the row
/// are the same document.
pub fn record_document(
    act: DecisionAct,
    change_id: &str,
    revision: &str,
    scope_version: u32,
    actor: &str,
    at: &str,
    exceptions: &[DecisionException],
) -> serde_json::Value {
    let exceptions = in_canonical_order(exceptions);
    serde_json::json!({
        "schema_version": DECISION_RECORD_SCHEMA_VERSION,
        /* DECISION-FILE-AFTER-WITHDRAW: the file is a copy, replaced when a
         * later act on its version is recorded; the ledger is the record. */
        "derived_copy": true,
        /* `B7.22`: the file a person reads without the tool says which of the
         * two this was, in its own field, rather than leaving them to notice
         * an `exceptions` list further down. */
        "act": record_act(act, &exceptions),
        "change_id": change_id,
        "version": revision,
        "accepted_criteria_version": scope_version,
        "actor": actor,
        "decided_at": at,
        "exceptions": exceptions,
        "note": record_note(act),
    })
}

/// What the file says it does and does not do, per act.
fn record_note(act: DecisionAct) -> &'static str {
    match act {
        DecisionAct::Approved => {
            "This records an approval. It does not merge, deploy or tell the agent anything."
        }
        DecisionAct::Withdrawn => {
            "This records a withdrawal. It does not merge, deploy or tell the agent anything."
        }
    }
}

/// [`record_document`] for a row already in the ledger, naming that row: the
/// exact document written for it (or that would have been).
pub fn record_document_for_row(change_id: &str, row: &DecisionRow) -> serde_json::Value {
    let mut doc = record_document(
        row.act,
        change_id,
        &row.target_revision,
        row.scope_version,
        &row.actor,
        &row.decided_at,
        &row.exceptions,
    );
    if let Some(fields) = doc.as_object_mut() {
        fields.insert("decision_id".into(), serde_json::json!(row.id));
        /* Only a withdrawal takes something back, so only its copy carries
         * the field; an approval's copy has no `withdraws` key at all. */
        if let Some(withdraws) = &row.withdraws {
            fields.insert("withdraws".into(), serde_json::json!(withdraws));
        }
    }
    doc
}

/// The schema-2 document an older release wrote for an approval row, exactly:
/// no `derived_copy`, no `decision_id`, no `withdraws`. Only the export reads
/// it, and only for a file that itself says `"schema_version": 2`, so an
/// untouched copy written before the upgrade still reads as unchanged and
/// any edit to it still reads as changed.
pub fn record_document_v2_for_row(change_id: &str, row: &DecisionRow) -> serde_json::Value {
    let exceptions = in_canonical_order(&row.exceptions);
    serde_json::json!({
        "schema_version": DECISION_RECORD_SCHEMA_VERSION_2,
        "act": record_act(row.act, &exceptions),
        "change_id": change_id,
        "version": row.target_revision,
        "accepted_criteria_version": row.scope_version,
        "actor": row.actor,
        "decided_at": row.decided_at,
        "exceptions": exceptions,
        "note": "This records an approval. It does not merge, deploy or tell the agent anything.",
    })
}

/// **DECISION-HISTORY-SHAPE-R2.** The `act` the decision record file says for
/// an act given under `exceptions` — the ONE rule, used by the copy writer (`stage_copy`) when
/// it writes the file and by every reader that compares a file with its row
/// (`change_export::decision_copy_standing`), so a reader never compares the
/// file with a narrower form than was written. An approval with exceptions is
/// `approved_with_exceptions` in the file and `approved` on the row
/// (`DecisionAct` has two acts and no third); the file says which in its own
/// field (`B7.22`).
pub fn record_act(act: DecisionAct, exceptions: &[DecisionException]) -> &'static str {
    match act {
        DecisionAct::Approved if !exceptions.is_empty() => "approved_with_exceptions",
        other => other.as_str(),
    }
}

/// A revision as a file name. Never a path, never a traversal.
fn safe_name(revision: &str) -> String {
    let cleaned: String = revision
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        "version".to_string()
    } else {
        cleaned
    }
}

/* ── what moved ────────────────────────────────────────────────────────── */

/// The files that differ between the version the screen was showing and the
/// version the record stands at now.
///
/// T-F-1-F6. There are two ways a file can differ, and until T-5-2 only one of
/// them was looked at:
///
///   * **Committed content.** An edit that has been committed leaves the
///     working tree clean again, so it appears in no snapshot manifest at all.
///     [`crate::snapshot::committed_changed_paths`] (T-F-1-F5) asks git itself
///     what differs between the two HEAD commits. That is the path the founder
///     test walks — edit, commit, prepare — and it used to produce a list
///     holding nothing but an internal marker.
///   * **Working-tree dirtiness.** Staged, unstaged, untracked and deleted, as
///     the two snapshots recorded them.
///
/// The answer is the union of the two, with the product's own `.thruline/`
/// folder excluded from the comparison (T-S-4-F3) and **no internal key ever
/// in it**: every entry is a repository-relative path a person can open.
///
/// An EMPTY list means the record cannot name them — git could not answer and
/// neither snapshot holds dirtiness that differs — and the caller says exactly
/// that rather than implying nothing moved.
pub fn changed_files(
    ledger: &Ledger,
    change_id: &str,
    seen: &str,
    now: &str,
) -> Result<Vec<String>> {
    let before = version_side(ledger, seen)?;
    let after = version_side(ledger, now)?;
    let mut changed: Vec<String> = Vec::new();

    /* The committed half. Both HEADs and a worktree that still exists, or
     * nothing at all: a question git cannot answer is answered as unknown,
     * never as "nothing changed". */
    if let (Some(before_head), Some(after_head)) = (&before.head, &after.head) {
        if let Some(root) = worktree_root(ledger, change_id)? {
            if root.is_dir() {
                if let Some(paths) =
                    crate::snapshot::committed_changed_paths(&root, before_head, after_head)
                {
                    changed.extend(paths.into_iter().filter(|p| !is_our_own(p)));
                }
            }
        }
    }

    /* The dirty half. */
    for (path, digest) in &after.dirty {
        if before.dirty.get(path) != Some(digest) {
            changed.push(path.clone());
        }
    }
    for path in before.dirty.keys() {
        if !after.dirty.contains_key(path) {
            changed.push(path.clone());
        }
    }

    changed.sort();
    changed.dedup();
    Ok(changed)
}

/// One side of the comparison: the commit it stands on, and the dirtiness the
/// snapshot recorded.
struct VersionSide {
    /// A recorded snapshot's git HEAD, or — when the version is a bare
    /// revision with no snapshot stored — the revision itself.
    head: Option<String>,
    /// Every path the snapshot recorded as dirty, with the digest it recorded.
    /// A deleted file carries no digest and is recorded as deleted, which still
    /// differs from the same path present.
    dirty: BTreeMap<String, String>,
}

fn version_side(ledger: &Ledger, version: &str) -> Result<VersionSide> {
    let Some((snapshot, _)) = crate::snapshot::load(ledger, version)? else {
        /* Not a snapshot this ledger holds. `standing_target` falls back to the
         * base revision, which IS a commit, so the committed half can still
         * answer. Dirtiness is unrecorded — which is not the same as clean, and
         * so contributes nothing to the comparison rather than a false
         * "everything was added". */
        let bare = version.trim();
        return Ok(VersionSide {
            head: (!bare.is_empty()).then(|| bare.to_string()),
            dirty: BTreeMap::new(),
        });
    };
    let mut dirty = BTreeMap::new();
    for group in [
        &snapshot.tree.staged,
        &snapshot.tree.unstaged,
        &snapshot.tree.untracked,
    ] {
        for entry in group {
            if is_our_own(&entry.path) {
                continue;
            }
            dirty.insert(
                entry.path.clone(),
                entry.sha256.clone().unwrap_or_else(|| "none".to_string()),
            );
        }
    }
    for path in &snapshot.tree.deleted {
        if is_our_own(path) {
            continue;
        }
        dirty.insert(path.clone(), "deleted".to_string());
    }
    Ok(VersionSide {
        head: snapshot
            .git
            .as_ref()
            .and_then(|g| g.head.clone())
            .filter(|h| !h.trim().is_empty()),
        dirty,
    })
}

/// Whether a path is ThruLine's own folder under the project.
///
/// T-S-4-F3. Approving writes `.thruline/decisions/<version>.json` into the
/// project, so on the next snapshot the approval's own artefact appeared in the
/// list of "files that changed since" — the product reporting its own footprint
/// back to a person as their edit. It is excluded from the COMPARISON, not
/// merely from the list that is printed: that way writing a decision cannot
/// move the version at all, rather than moving it invisibly.
pub(crate) fn is_our_own(path: &str) -> bool {
    let p = path.trim_start_matches("./");
    p == ".thruline" || p.starts_with(".thruline/")
}

/// Where the decision record for one version would be written, for a caller
/// that wants to say so before anything is written.
pub fn record_path_for(root: &Path, revision: &str) -> PathBuf {
    root.join(DECISIONS_DIR)
        .join(format!("{}.json", safe_name(revision)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_revision_never_becomes_a_path() {
        assert_eq!(safe_name("../../etc/passwd"), "etcpasswd");
        assert_eq!(safe_name("e71c5b0"), "e71c5b0");
        assert_eq!(safe_name("///"), "version");
    }

    #[test]
    fn an_act_round_trips_through_its_word() {
        for act in [DecisionAct::Approved, DecisionAct::Withdrawn] {
            assert_eq!(DecisionAct::parse(act.as_str()).unwrap(), act);
        }
        assert!(DecisionAct::parse("amended").is_err());
    }

    fn exception(obligation: &str, rationale: &str) -> DecisionException {
        DecisionException {
            waiver_id: format!("wv_{obligation}"),
            obligation_id: obligation.to_string(),
            kind: "waived".to_string(),
            actor: "sam".to_string(),
            authority: "the release policy".to_string(),
            rationale: rationale.to_string(),
        }
    }

    /// DECISION-HISTORY-SHAPE-R4. The number in the id is a number, the review
    /// obligation follows the numbered lines, and an id that is not either sorts
    /// after them by name — including ids that only look numeric. Two spellings
    /// of one number (`02`, `2`) tie on the number and fall to the name.
    #[test]
    fn exceptions_are_ordered_by_number_then_review_then_the_rest_by_name() {
        let expected = [
            "criterion:1",
            "criterion:02",
            "criterion:2",
            "criterion:9",
            "criterion:10",
            "criterion:11",
            "review",
            "criterion:",
            "criterion:+3",
            "criterion:99999999999999999999999",
            "criterion:x",
        ];
        let sorted: Vec<DecisionException> = expected.iter().map(|o| exception(o, "why")).collect();
        let ids = |list: &[DecisionException]| -> Vec<String> {
            list.iter().map(|e| e.obligation_id.clone()).collect()
        };
        // Any starting order gives the one order.
        let mut reversed = sorted.clone();
        reversed.reverse();
        assert_eq!(ids(&in_canonical_order(&reversed)), ids(&sorted));
        for turn in 0..sorted.len() {
            let mut rotated = sorted.clone();
            rotated.rotate_left(turn);
            assert_eq!(ids(&in_canonical_order(&rotated)), ids(&sorted), "{turn}");
        }
        // Two exceptions on one obligation are still ordered, by their own
        // fields, so the result never depends on how they were held.
        let pair = [exception("criterion:2", "b"), exception("criterion:2", "a")];
        let flipped = [pair[1].clone(), pair[0].clone()];
        assert_eq!(in_canonical_order(&pair), in_canonical_order(&flipped));
    }
}
