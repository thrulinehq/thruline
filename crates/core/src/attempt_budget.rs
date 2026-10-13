//! T-4-4-ATTEMPT-BUDGET (R-2026-09-28-43; rehearsal Q14 C8; founder ruling
//! #22 (ii) a) — how many repair attempts a person allowed one Change, how
//! many have been made, and the stop once they are spent.
//!
//! ## What an attempt is
//!
//! The manual repair cycle is: a check shows a problem; somebody changes the
//! code; the check is run again. ThruLine already keeps every run
//! (`change_run_executions`, append-only) and the version each one examined
//! (`examined_inputs_id`, frozen at the spawn and computed from the content of
//! the project folder — never the snapshot id the Change shows, which moves on
//! every run, PW-F11). So an attempt is read off the runs, never kept as a
//! second counter:
//!
//! > **an attempt is a version first examined by a check run of this Change
//! > recorded after the budget was set.**
//!
//! A re-run of a version already examined is not a new attempt; a version
//! examined before the budget was set never counts; a check that never
//! started (`blocked`) ran nothing and is not an attempt. A run whose version
//! the record does not hold counts as an attempt of its own — the count errs
//! toward stopping, never toward running more than a person allowed. A commit
//! nobody has run a check on is not counted: nothing here reads the project
//! folder or its history.
//!
//! "After" is `rowid` order in `change_run_executions`, anchored on the row the
//! budget names in `after_execution`; the standing budget is the last row
//! appended to `change_attempt_budgets`. Never a clock.
//!
//! The anchor is the last **ended** execution row of this Change (any state
//! but `running`) appended before the budget row — never a `running` row
//! (T-4-4-ATTEMPT-BUDGET-R4). A `running` row is written the moment a check is
//! admitted, long before the check ends; anchoring on it made a check that was
//! in flight when a person set the budget part of "before", so it was never
//! counted and one more check ran than the person allowed. A run is "before
//! the budget" only if it had **ended** at or before the anchor; a run in
//! flight when the budget was set counts against it once it has examined a
//! new version, by the same rule as any other. This also reads a budget row
//! written before this rule (one anchored on a `running` row) the same way. A
//! `running` row whose run never records an end is read as in flight, so it
//! counts: the count errs toward stopping.
//!
//! ## The stop
//!
//! Once every allowed attempt is made, [`admits`] refuses a run on a NEW
//! version, and `thruline check --for-change` asks it before the spawn — both
//! doors (the terminal and the run route) go through that one wrapper. A run on
//! a version already examined is admitted.
//!
//! The asking and the recording are one act (T-4-4-ATTEMPT-BUDGET-R2): the
//! wrapper calls [`admit`], which takes the store's write lock (`BEGIN
//! IMMEDIATE`) before it reads, and holds it — as a [`Hold`] — until the
//! wrapper's `running` row is written and [`Hold::commit`] releases it. The
//! lock is SQLite's own, on the store file, so every process and every door
//! shares it: a second caller cannot read the record between the first
//! caller's "yes" and the first caller's row, and when it reads, the first
//! caller's attempt is there and counted. A caller that cannot get the lock
//! within the connection's busy wait is refused, in plain words, having
//! written nothing and started nothing: fail closed, never a stall. Nothing is deleted and nothing is
//! re-labelled; a person sets a further budget to go on, and the earlier one
//! stays readable beside it.
//!
//! ## What this is not
//!
//! Not a result. Nothing here moves a verdict, a coverage count, a readiness
//! answer or an approval, and nothing here runs anything. The one sentence that
//! says "unresolved" is said only when the caller passes the readiness answer
//! it was served and that answer is not Ready ([`sentences`]).

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::error::Result;
use crate::ledger::Ledger;

/// The one table this module writes.
pub const TABLE: &str = "change_attempt_budgets";

/// The largest budget one act may set; the store refuses more too.
pub const MAX_BUDGET: u32 = 99;

/// One person's budget, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Budget {
    pub id: String,
    pub change_id: String,
    /// How many repair attempts were allowed.
    pub budget: u32,
    pub actor: String,
    /// The last execution record of a run that had ENDED (any state but
    /// `running`) when the budget was set, or `None` when no run had ended.
    /// A run counts unless it had ended at or before this row, so a check
    /// still running when the budget was set is counted against it.
    pub after_execution: Option<String>,
    /// The version that execution examined: the version the budget was set on.
    pub set_on_version: Option<String>,
    pub recorded_at: String,
}

/// Why a budget was not recorded. Nothing is written in any of these cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    NoSuchChange,
    NoActor,
    /// Zero, or more than [`MAX_BUDGET`].
    OutOfRange,
}

impl Refusal {
    /// The sentence a person reads. Nothing was saved in any case.
    pub fn plain(self, change_id: &str) -> String {
        match self {
            Refusal::NoSuchChange => format!(
                "No Change with id {change_id:?} is recorded in this store. No budget was saved."
            ),
            Refusal::NoActor => "A budget is a person's decision: say who is setting it with \
                                 --actor <name>. No budget was saved."
                .to_string(),
            Refusal::OutOfRange => format!(
                "A budget is between 1 and {MAX_BUDGET} repair attempts. No budget was saved."
            ),
        }
    }
}

/// One attempt: one version, and every run that examined it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attempt {
    /// 1-based, in the order the versions were first examined.
    pub number: u32,
    /// The version, as the runs froze it. `None` when the record does not say.
    pub examined_inputs_id: Option<String>,
    /// When the first run on this version started, as recorded.
    pub first_started_at: Option<String>,
    /// The runs, by run key, oldest first. Every one is kept.
    pub runs: Vec<String>,
    /// What became of each run, as its execution record says (`completed`,
    /// `cancelled`, `interrupted`, `running`), in the same order.
    pub states: Vec<String>,
}

impl Attempt {
    /// One sentence: which attempt, which version, when, and its runs.
    pub fn line(&self) -> String {
        let version = self
            .examined_inputs_id
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| "a version the record does not name".to_string());
        let when = self
            .first_started_at
            .as_deref()
            .map(|at| format!(", first checked {at}"))
            .unwrap_or_default();
        let runs = self
            .runs
            .iter()
            .zip(&self.states)
            .map(|(key, state)| format!("{key} ({state})"))
            .collect::<Vec<_>>()
            .join(", ");
        let count = if self.runs.len() == 1 {
            "1 run".to_string()
        } else {
            format!("{} runs", self.runs.len())
        };
        format!(
            "Attempt {} (version {version}{when}): {count}: {runs}.",
            self.number
        )
    }
}

/// The standing budget of one Change and every attempt made against it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// The last budget appended: the one that stands.
    pub budget: Budget,
    /// Every earlier budget, oldest first. Superseded, never deleted.
    pub earlier: Vec<Budget>,
    pub attempts: Vec<Attempt>,
    pub used: u32,
    pub remaining: u32,
    /// Every allowed attempt is made; a new version will not be started.
    pub spent: bool,
    /// Every version already examined (before the budget or by an attempt): a
    /// run on one of these is not a new attempt.
    #[serde(skip)]
    seen: BTreeSet<String>,
}

fn attempts_word(n: u32) -> &'static str {
    if n == 1 {
        "attempt"
    } else {
        "attempts"
    }
}

fn to_allow_more(change_id: &str) -> String {
    format!("To allow more: `thruline change attempts {change_id} --budget <n> --actor <name>`.")
}

fn mint() -> String {
    format!("ab_{}", ulid::Ulid::generate())
}

/// Record one person's budget for one Change: one appended row.
///
/// `Ok(Err(refusal))` means nothing was written, and says why.
pub fn set(
    ledger: &Ledger,
    change_id: &str,
    budget: u32,
    actor: &str,
    at: &str,
) -> Result<std::result::Result<Budget, Refusal>> {
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
    if budget == 0 || budget > MAX_BUDGET {
        return Ok(Err(Refusal::OutOfRange));
    }
    // The anchor and the row under one write lock: a run admitted while the
    // budget is being set is either wholly before the anchor or wholly after.
    // The anchor is the last ENDED row, never a `running` one: a check in
    // flight now has not ended, so it counts against this budget (R4).
    let transaction = rusqlite::Transaction::new_unchecked(
        ledger.conn(),
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let anchor: Option<(String, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT id, examined_inputs_id FROM change_run_executions
              WHERE change_id=?1 AND state <> 'running'
              ORDER BY rowid DESC LIMIT 1",
            params![change_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (after_execution, set_on_version) = match anchor {
        Some((id, version)) => (Some(id), version),
        None => (None, None),
    };
    let row = Budget {
        id: mint(),
        change_id: change_id.to_string(),
        budget,
        actor: actor.to_string(),
        after_execution,
        set_on_version,
        recorded_at: at.to_string(),
    };
    ledger.conn().execute(
        "INSERT INTO change_attempt_budgets(id, change_id, budget, actor, after_execution,
             set_on_version, recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            row.id,
            row.change_id,
            row.budget,
            row.actor,
            row.after_execution,
            row.set_on_version,
            row.recorded_at,
        ],
    )?;
    transaction.commit()?;
    Ok(Ok(row))
}

fn budgets(ledger: &Ledger, change_id: &str) -> Result<Vec<Budget>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, change_id, budget, actor, after_execution, set_on_version, recorded_at
           FROM change_attempt_budgets WHERE change_id=?1 ORDER BY rowid",
    )?;
    let rows = statement.query_map(params![change_id], |row| {
        Ok(Budget {
            id: row.get(0)?,
            change_id: row.get(1)?,
            budget: row.get(2)?,
            actor: row.get(3)?,
            after_execution: row.get(4)?,
            set_on_version: row.get(5)?,
            recorded_at: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// One run, folded from its rows.
struct RunFold {
    first_rowid: i64,
    /// The first row that says how the run ended, if one is recorded.
    ended_rowid: Option<i64>,
    version: Option<String>,
    started_at: Option<String>,
    state: String,
}

/// Where this Change's standing budget is, or `None` when nobody set one.
///
/// Reads `change_attempt_budgets` and `change_run_executions`, and nothing
/// else.
pub fn status(ledger: &Ledger, change_id: &str) -> Result<Option<Status>> {
    let mut all = budgets(ledger, change_id)?;
    let Some(budget) = all.pop() else {
        return Ok(None);
    };
    let anchor: i64 = match &budget.after_execution {
        None => 0,
        Some(id) => ledger
            .conn()
            .query_row(
                "SELECT rowid FROM change_run_executions WHERE id=?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            // The table is append-only, so the row cannot be gone; if it ever
            // is, count from the start, so the count errs toward stopping.
            .unwrap_or(0),
    };

    let mut statement = ledger.conn().prepare(
        "SELECT rowid, run_key, state, examined_inputs_id, started_at
           FROM change_run_executions WHERE change_id=?1 ORDER BY rowid",
    )?;
    let rows = statement.query_map(params![change_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut runs: BTreeMap<String, RunFold> = BTreeMap::new();
    for row in rows {
        let (rowid, key, state, version, started) = row?;
        let fold = runs.entry(key).or_insert(RunFold {
            first_rowid: rowid,
            ended_rowid: None,
            version: None,
            started_at: None,
            state: String::new(),
        });
        if fold.version.is_none() {
            fold.version = version;
        }
        if fold.started_at.is_none() {
            fold.started_at = started;
        }
        if state != "running" && fold.ended_rowid.is_none() {
            fold.ended_rowid = Some(rowid);
        }
        fold.state = state;
    }
    drop(statement);
    let mut ordered: Vec<(String, RunFold)> = runs.into_iter().collect();
    ordered.sort_by_key(|(_, f)| f.first_rowid);

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut attempts: Vec<Attempt> = Vec::new();
    for (key, fold) in ordered {
        // A check that never started ran nothing.
        if fold.state == "blocked" {
            continue;
        }
        let version_key = fold.version.clone().unwrap_or_else(|| format!("run:{key}"));
        // Before the budget only if it had ENDED by the anchor; a run still in
        // flight when the budget was set is counted (R4).
        if fold.ended_rowid.is_some_and(|ended| ended <= anchor) {
            seen.insert(version_key);
            continue;
        }
        let existing = fold.version.as_deref().and_then(|v| {
            attempts
                .iter()
                .position(|a| a.examined_inputs_id.as_deref() == Some(v))
        });
        let index = match existing {
            Some(i) => i,
            // A version examined before the budget was set.
            None if seen.contains(&version_key) => continue,
            None => {
                seen.insert(version_key);
                attempts.push(Attempt {
                    number: attempts.len() as u32 + 1,
                    examined_inputs_id: fold.version.clone(),
                    first_started_at: fold.started_at.clone(),
                    runs: Vec::new(),
                    states: Vec::new(),
                });
                attempts.len() - 1
            }
        };
        attempts[index].runs.push(key);
        attempts[index].states.push(fold.state);
    }

    let used = attempts.len() as u32;
    Ok(Some(Status {
        remaining: budget.budget.saturating_sub(used),
        spent: used >= budget.budget,
        budget,
        earlier: all,
        attempts,
        used,
        seen,
    }))
}

/// The limit of the count, in the words every door says it
/// (T-4-4-ATTEMPT-BUDGET-R3, P-2026-09-29-3). A version is told apart by
/// `examined_inputs_id`, a hash over the whole project folder, `.thruline/`
/// included, so a change only to ThruLine's own files there (an approval's
/// decision file, a receipt) is a new version and spends an attempt. It errs
/// toward stopping, never toward allowing more. The fix is card
/// EXAMINED-INPUTS-OWN-FILES; until it lands this sentence is said by
/// [`sentences`] and at the end of every refusal [`admits`] gives, and is
/// the `--help` of `thruline change attempts`.
pub const OWN_FILES_LIMIT: &str = "A change made only to ThruLine's own files in the project \
     (under .thruline/) currently counts as a new attempt.";

/// May a check of this Change on `examined_inputs_id` start?
///
/// `None` means yes. `Some(sentence)` means no, and is what the wrapper says
/// before refusing the spawn: every allowed attempt is made and this would be
/// a new one.
pub fn admits(
    ledger: &Ledger,
    change_id: &str,
    examined_inputs_id: &str,
) -> Result<Option<String>> {
    let Some(s) = status(ledger, change_id)? else {
        return Ok(None);
    };
    if !s.spent || s.seen.contains(examined_inputs_id) {
        return Ok(None);
    }
    let n = s.budget.budget;
    Ok(Some(format!(
        "no check was started: {} allowed {n} repair {} on Change {change_id} and {} {} been \
         made, and this check is on a version no check of this Change has examined, so it \
         would be a further attempt. A check on a version already examined still runs. {} {}",
        s.budget.actor,
        attempts_word(n),
        s.used,
        if s.used == 1 { "has" } else { "have" },
        to_allow_more(change_id),
        OWN_FILES_LIMIT
    )))
}

/// What [`admit`] decided.
pub enum Admission<'l> {
    /// The check may start. The store's write lock is held until the caller
    /// has written its `running` row and called [`Hold::commit`]; dropping the
    /// hold instead rolls back and releases it, having written nothing.
    Admitted(Hold<'l>),
    /// The check may not start, and why, in the words the wrapper says. The
    /// lock is already released and nothing was written.
    Refused(String),
}

/// The write lock an admitted check holds between "yes" and its own row.
pub struct Hold<'l> {
    transaction: rusqlite::Transaction<'l>,
}

impl Hold<'_> {
    /// Release the lock, keeping what was written under it — the admitted
    /// check's `running` row, which is the attempt the next caller counts.
    pub fn commit(self) -> Result<()> {
        Ok(self.transaction.commit()?)
    }
}

/// [`admits`], decided under the store's write lock and returned with it held.
///
/// Every caller of a check of a Change goes through here, budget or not: the
/// lock is what makes the decision and the row the decision allows one act.
/// Only a caller that could not get the lock within the connection's busy
/// wait is refused for that reason; any other failure is an error, which the
/// wrapper also refuses on.
pub fn admit<'l>(
    ledger: &'l Ledger,
    change_id: &str,
    examined_inputs_id: &str,
) -> Result<Admission<'l>> {
    let transaction = match rusqlite::Transaction::new_unchecked(
        ledger.conn(),
        rusqlite::TransactionBehavior::Immediate,
    ) {
        Ok(transaction) => transaction,
        Err(rusqlite::Error::SqliteFailure(e, _))
            if matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            return Ok(Admission::Refused(busy(change_id)));
        }
        Err(e) => return Err(e.into()),
    };
    match admits(ledger, change_id, examined_inputs_id)? {
        None => Ok(Admission::Admitted(Hold { transaction })),
        Some(stop) => {
            drop(transaction);
            Ok(Admission::Refused(stop))
        }
    }
}

/// The refusal when another caller holds the store's write lock too long.
fn busy(change_id: &str) -> String {
    format!(
        "no check was started: the record of Change {change_id} was busy (another check, or \
         another write to this store, held it) and did not come free in time, so whether this \
         check would be a further repair attempt could not be decided. Nothing was recorded. \
         Run it again."
    )
}

/// Everything a person reads about this Change's budget, as one list of
/// sentences: the budget, every attempt, the limit of the count, and last the
/// sentence that says whether another attempt may be made.
///
/// `ready` is the readiness answer the caller was **served** (`Some(true)` is
/// Ready), or `None` when it was not read. It is never derived here, and it
/// only chooses the words of the last sentence: "unresolved" is said only on
/// `Some(false)`.
///
/// A later sentence (FOUNDER-ASKS #21: what the project folder holds now) is
/// meant to be inserted before the last one by its caller; it can never add
/// an attempt or change the stop, which [`status`] and [`admits`] decide from
/// the record alone.
pub fn sentences(change_id: &str, status: Option<&Status>, ready: Option<bool>) -> Vec<String> {
    let Some(s) = status else {
        return vec![format!(
            "No attempt budget is set for this Change, so nothing limits how many versions are \
             checked. To set one: `thruline change attempts {change_id} --budget <n> --actor \
             <name>`."
        )];
    };
    let n = s.budget.budget;
    let who = &s.budget.actor;
    let mut out = vec![format!(
        "{who} allowed {n} repair {} on this Change (set {}).",
        attempts_word(n),
        s.budget.recorded_at
    )];
    out.extend(s.attempts.iter().map(Attempt::line));
    out.push(
        "An attempt is a version of the project first examined by a check of this Change after \
         the budget was set; a commit no check has run on is not counted."
            .to_string(),
    );
    out.push(OWN_FILES_LIMIT.to_string());
    if !s.spent {
        out.push(format!("{} made, {} left.", s.used, s.remaining));
        return out;
    }
    let made = if n == 1 {
        format!("The one repair attempt {who} allowed has been made")
    } else {
        format!("All {n} repair attempts {who} allowed have been made")
    };
    let standing = match ready {
        Some(false) => "; this Change is unresolved: its readiness answer is not Ready",
        Some(true) => "; its readiness answer says Ready — see it on the Change",
        None => "",
    };
    out.push(format!(
        "{made}{standing}. No check will be started on a new version until someone allows \
         more. {}",
        to_allow_more(change_id)
    ));
    out
}
