//! C30 R-LEDGER — pausing recording, and excluding a path, are a person's acts,
//! so they are in the record.
//!
//! One row per act (`paused`, `resumed`, `excluded`, `included`), append-only,
//! on its own machine-wide hash chain: scope `capture_hold`, the construction
//! `run_permission` uses. `content_sha256` is the sha256 of the row's own
//! columns in [`content`]'s canonical encoding, `chain_hash` links it to the row
//! before, and [`verify_chain`] re-folds the whole chain from the rows alone, so
//! `thruline verify` can say whether a pause the product shows is a pause the
//! record adds up. A resumption or an inclusion is ITS OWN ROW pointing at the
//! act it ends (`ends`); nothing is edited and nothing is deleted.
//!
//! WHAT THIS DOES AND DOES NOT DO. It records an act and answers where the
//! record stands ([`standing`]) and whether a path is under a pause in force
//! ([`standing_for`]). It does not itself stop anything being recorded:
//! honouring a pause is the six capture callers' job (`capture_pause`), and
//! this module touches no capture path, no mirror and no raw line. It names a
//! project and a folder, never a Change. Nothing here is a result.
//!
//! THE DEFAULTS. `env` and `secrets` are excluded until a person turns them off
//! (`generated` is not), and they are code defaults, not rows (C34 ruling Q7):
//! [`holds_view`] serves them as "on by default (not recorded)". The first change
//! to one appends, through [`record_for_person`]. [`record`] is the chain's own
//! writer and knows only rows.
//!
//! An act that changes nothing is refused rather than recorded (`already_paused`,
//! `not_paused`, `already_excluded`, `not_excluded`): the record then says each
//! thing a person did once, and "paused by you, 16:10 - now" has one row behind
//! it.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// The chain's scope: its genesis is `sha256("capture_hold")`.
pub const CHAIN_SCOPE: &str = "capture_hold";

/// The table, named once for the screens that cite a row by it.
pub const TABLE: &str = "capture_holds";

/// The longest pattern the table admits (its CHECK says the same).
pub const MAX_PATTERN_CHARS: usize = 256;

/// The four acts, and there is no fifth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Act {
    Paused,
    Resumed,
    Excluded,
    Included,
}

impl Act {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paused => "paused",
            Self::Resumed => "resumed",
            Self::Excluded => "excluded",
            Self::Included => "included",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "paused" => Ok(Self::Paused),
            "resumed" => Ok(Self::Resumed),
            "excluded" => Ok(Self::Excluded),
            "included" => Ok(Self::Included),
            other => Err(CoreError::Integrity(format!(
                "a capture hold act must be paused, resumed, excluded or included, not {other:?}"
            ))),
        }
    }

    /// Whether the act names a path pattern (and so is about an exclusion).
    pub fn names_a_pattern(self) -> bool {
        matches!(self, Self::Excluded | Self::Included)
    }
}

/// One row, exactly as the table holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HoldRow {
    pub id: String,
    pub seq: i64,
    pub act: Act,
    pub project_id: String,
    pub project_folder: String,
    /// `excluded` and `included` only.
    pub pattern: Option<String>,
    pub actor: String,
    pub recorded_at: String,
    /// On a resumption, the pause it ends; on an inclusion, the exclusion it
    /// ends.
    pub ends: Option<String>,
    pub content_sha256: String,
    pub prev_hash: String,
    pub chain_hash: String,
}

/// What one act says, before it has a place on the chain.
#[derive(Debug, Clone)]
pub struct Hold<'a> {
    pub project_id: &'a str,
    pub project_folder: &'a str,
    pub actor: &'a str,
    pub recorded_at: &'a str,
}

/// Why an act was not recorded: it would have changed nothing. Each word is the
/// code a route answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    AlreadyPaused,
    NotPaused,
    AlreadyExcluded,
    NotExcluded,
}

impl Refusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyPaused => "already_paused",
            Self::NotPaused => "not_paused",
            Self::AlreadyExcluded => "already_excluded",
            Self::NotExcluded => "not_excluded",
        }
    }

    /// What a person is told, by the route and by the terminal alike. It says
    /// nothing was saved, because nothing was.
    pub fn sentence(self) -> &'static str {
        match self {
            Self::AlreadyPaused => "This project is already paused, so nothing was saved.",
            Self::NotPaused => "This project is not paused, so nothing was saved.",
            Self::AlreadyExcluded => "That path is already excluded, so nothing was saved.",
            Self::NotExcluded => "That path is not excluded, so nothing was saved.",
        }
    }
}

/// What a person is told when a path cannot be excluded: `why` is a clause from
/// [`pattern_problem`].
pub fn bad_pattern_sentence(why: &str) -> String {
    format!(
        "That cannot be left out: {why}. Use a built-in name (env, secrets, generated) or a \
         path inside the project. Nothing was saved."
    )
}

/// What came of one [`record`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// This call appended the row.
    Appended(Box<HoldRow>),
    /// Nothing was appended, because the act would have changed nothing.
    Refused(Refusal),
}

/// Where the record stands for one folder.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Standing {
    /// The pause in force, when the folder's latest pause/resume act is a pause.
    pub paused: Option<HoldRow>,
    /// Every exclusion in force, oldest first.
    pub exclusions: Vec<HoldRow>,
}

/// The canonical content a row's `content_sha256` is taken over: its own
/// columns, as a JSON array of strings. JSON string escaping makes the encoding
/// injective — no two different rows share a pre-image, whatever a folder, a
/// pattern or an actor holds. An absent pattern or `ends` is the empty string;
/// the act is in the content, and the table's CHECKs make the two unambiguous.
pub fn content(
    id: &str,
    act: Act,
    hold: &Hold<'_>,
    pattern: Option<&str>,
    ends: Option<&str>,
) -> Result<String> {
    Ok(serde_json::to_string(&[
        "capture_hold_v1",
        id,
        act.as_str(),
        hold.project_id,
        hold.project_folder,
        pattern.unwrap_or(""),
        hold.actor,
        hold.recorded_at,
        ends.unwrap_or(""),
    ])?)
}

fn require(value: &str, what: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "a capture hold must name its {what}"
        )));
    }
    Ok(())
}

/// Why a path pattern cannot be excluded, as a clause that follows "it": `None`
/// when it can. A path pattern a person may exclude is a built-in key (`env`,
/// `secrets`, `generated`) or one relative path pattern: not empty, at most
/// [`MAX_PATTERN_CHARS`], not absolute, no `..` component, no control
/// character, and `*` the only wildcard.
pub fn pattern_problem(pattern: &str) -> Option<&'static str> {
    if pattern.trim().is_empty() {
        return Some("it is empty");
    }
    if pattern.chars().count() > MAX_PATTERN_CHARS {
        return Some("it is longer than 256 characters");
    }
    if pattern.starts_with('/') || pattern.starts_with('\\') {
        return Some("it is an absolute path");
    }
    if pattern.split(['/', '\\']).any(|part| part == "..") {
        return Some("it climbs out of the project with `..`");
    }
    if pattern.chars().any(char::is_control) {
        return Some("it holds a control character");
    }
    if pattern
        .chars()
        .any(|c| matches!(c, '?' | '[' | ']' | '{' | '}'))
    {
        return Some("`*` is the only wildcard");
    }
    None
}

/// [`pattern_problem`] as the error the writer refuses with.
pub fn check_pattern(pattern: &str) -> Result<()> {
    match pattern_problem(pattern) {
        Some(why) => Err(CoreError::Config(format!(
            "an excluded path must be a built-in name or a relative pattern: {why}"
        ))),
        None => Ok(()),
    }
}

/// The actor on the row that writes a built-in default down. A default is
/// ThruLine's, not a person's, so the one row that has to exist before a person
/// can turn a default off does not say a person turned it on.
pub const DEFAULT_ACTOR: &str = "ThruLine default";

/// A name ThruLine knows how to leave out without being given a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltIn {
    /// What is typed and recorded: `env`.
    pub key: &'static str,
    /// What it covers, in words a person recognises.
    pub covers: &'static str,
    /// One short line under the name.
    pub note: &'static str,
    /// On until a person turns it off. Not a row: a default lives in the code
    /// until the first change (C34 ruling Q7).
    pub on_by_default: bool,
}

/// The three built-in names, in the order Settings lists them. `env` and
/// `secrets` are on by default and `generated` is not, as the canvas draws
/// them (the first two ticked, the third not).
pub const BUILT_INS: [BuiltIn; 3] = [
    BuiltIn {
        key: "env",
        covers: ".env and .env.* files",
        note: "environment files",
        on_by_default: true,
    },
    BuiltIn {
        key: "secrets",
        covers: "secrets/",
        note: "the whole folder",
        on_by_default: true,
    },
    BuiltIn {
        key: "generated",
        covers: "generated files",
        note: "files your repo marks as generated",
        on_by_default: false,
    },
];

/// The built-in a pattern names, when it is exactly one of the three keys. A
/// folder that happens to be called `env` is written `env/`.
pub fn built_in(pattern: &str) -> Option<&'static BuiltIn> {
    BUILT_INS.iter().find(|b| b.key == pattern)
}

/// What the product says in every place a pause or an exclusion is recorded
/// (C34 ruling Q5, Q4): a pause is honoured by the six capture callers, and an
/// exclusion is applied when a view is shown or exported. Said in both halves, because a
/// switch that reads "paused" must be a claim the record backs, and a path
/// listed as excluded must not be read as left out of the sealed copy.
pub const HONOURED_NOTE: &str = "While paused, ThruLine reads nothing from this folder, and a session started while it was paused is never read. A path exclusion leaves matching entries out when ThruLine shows or exports a view. It does not stop recording, and ThruLine's sealed copy of each transcript still holds what Claude Code wrote.";

/// Append one act on the chain, in one IMMEDIATE transaction so two writers
/// cannot take the same place, reading the folder's standing inside the same
/// transaction so two presses cannot both pause, or both end one pause.
///
/// `pattern` is required for [`Act::Excluded`] and [`Act::Included`] and refused
/// for the other two. An act that would change nothing appends nothing and comes
/// back as [`Recorded::Refused`].
///
/// This is the chain's own writer: it knows only the rows. A built-in default
/// (`env`, `secrets`) that has no row is, here, not excluded. The product's
/// doors (the routes, the terminal) call [`record_for_person`] instead.
pub fn record(
    ledger: &Ledger,
    act: Act,
    hold: &Hold<'_>,
    pattern: Option<&str>,
) -> Result<Recorded> {
    record_inner(ledger, act, hold, pattern, false)
}

/// [`record`] for a person's act as the product offers it: the built-in defaults
/// count as the exclusions they are (C34 ruling Q7).
///
/// - Excluding a default that has never had a row is `already_excluded`: it is
///   on, so the act would change nothing.
/// - Including (turning off) a default that has never had a row is the first
///   change to it, and it appends. The table cannot say "included" without an
///   exclusion to end, so the transaction first writes the default down as an
///   `excluded` row under [`DEFAULT_ACTOR`] and then appends the person's
///   `included` row ending it. Two rows, one transaction, the same time; the
///   person is named on the second only.
/// - Once a key has any row in the folder, the record decides it and the
///   default is never consulted again.
pub fn record_for_person(
    ledger: &Ledger,
    act: Act,
    hold: &Hold<'_>,
    pattern: Option<&str>,
) -> Result<Recorded> {
    record_inner(ledger, act, hold, pattern, true)
}

fn record_inner(
    ledger: &Ledger,
    act: Act,
    hold: &Hold<'_>,
    pattern: Option<&str>,
    defaults: bool,
) -> Result<Recorded> {
    for (value, what) in [
        (hold.project_id, "project"),
        (hold.project_folder, "project folder"),
        (hold.actor, "actor"),
        (hold.recorded_at, "time"),
    ] {
        require(value, what)?;
    }
    let pattern = match (act.names_a_pattern(), pattern) {
        (true, Some(p)) => {
            // What is checked is what is stored: a leading space must not hide
            // a `/` or a `..` from the check and then be trimmed off.
            let p = p.trim();
            check_pattern(p)?;
            Some(p)
        }
        (true, None) => {
            return Err(CoreError::Config(
                "an exclusion must name the path it is about".to_string(),
            ))
        }
        (false, None) => None,
        (false, Some(_)) => {
            return Err(CoreError::Config(
                "a pause or a resumption is about the whole folder and names no path".to_string(),
            ))
        }
    };
    let conn = ledger.conn();
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<Recorded> {
        let rows = history(ledger, hold.project_folder)?;
        let now = replay(&rows);
        // On in the code and not yet in the record: a default that is on, for a
        // key no row of this folder has ever named.
        let default_on = defaults
            && pattern.and_then(built_in).is_some_and(|b| b.on_by_default)
            && !rows.iter().any(|r| r.pattern.as_deref() == pattern);
        let appended = match act {
            Act::Paused => {
                if now.paused.is_some() {
                    return Ok(Recorded::Refused(Refusal::AlreadyPaused));
                }
                append(conn, act, hold, None, None)?
            }
            Act::Resumed => match &now.paused {
                Some(pause) => append(conn, act, hold, None, Some(&pause.id))?,
                None => return Ok(Recorded::Refused(Refusal::NotPaused)),
            },
            Act::Excluded => {
                if default_on
                    || now
                        .exclusions
                        .iter()
                        .any(|e| e.pattern.as_deref() == pattern)
                {
                    return Ok(Recorded::Refused(Refusal::AlreadyExcluded));
                }
                append(conn, act, hold, pattern, None)?
            }
            Act::Included => {
                match now
                    .exclusions
                    .iter()
                    .find(|e| e.pattern.as_deref() == pattern)
                {
                    Some(exclusion) => append(conn, act, hold, pattern, Some(&exclusion.id))?,
                    None if default_on => {
                        let written_down = append(
                            conn,
                            Act::Excluded,
                            &Hold {
                                actor: DEFAULT_ACTOR,
                                ..hold.clone()
                            },
                            pattern,
                            None,
                        )?;
                        append(conn, act, hold, pattern, Some(&written_down.id))?
                    }
                    None => return Ok(Recorded::Refused(Refusal::NotExcluded)),
                }
            }
        };
        Ok(Recorded::Appended(Box::new(appended)))
    })();
    match outcome {
        Ok(recorded) => {
            conn.execute_batch("COMMIT")?;
            Ok(recorded)
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// Put one row at the head of the chain. The caller holds the write
/// transaction, so the head it reads is the head this row follows.
fn append(
    conn: &rusqlite::Connection,
    act: Act,
    hold: &Hold<'_>,
    pattern: Option<&str>,
    ends: Option<&str>,
) -> Result<HoldRow> {
    let head: Option<(String, i64)> = conn
        .query_row(
            "SELECT chain_hash, seq FROM capture_holds ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (prev_hash, seq) = match head {
        Some((hash, last)) => (hash, last + 1),
        None => (genesis_prev_hash(CHAIN_SCOPE), 0),
    };
    let id = format!("chd_{}", ulid::Ulid::generate());
    let content_sha256 = sha256_hex(content(&id, act, hold, pattern, ends)?.as_bytes());
    let chain = chain_hash(&prev_hash, &content_sha256, CHAIN_SCOPE, seq);
    conn.execute(
        "INSERT INTO capture_holds(
             id, seq, act, project_id, project_folder, pattern, actor,
             recorded_at, ends, content_sha256, prev_hash, chain_hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            id,
            seq,
            act.as_str(),
            hold.project_id,
            hold.project_folder,
            pattern,
            hold.actor,
            hold.recorded_at,
            ends,
            content_sha256,
            prev_hash,
            chain,
        ],
    )?;
    Ok(HoldRow {
        id,
        seq,
        act,
        project_id: hold.project_id.to_string(),
        project_folder: hold.project_folder.to_string(),
        pattern: pattern.map(str::to_string),
        actor: hold.actor.to_string(),
        recorded_at: hold.recorded_at.to_string(),
        ends: ends.map(str::to_string),
        content_sha256,
        prev_hash,
        chain_hash: chain,
    })
}

const COLUMNS: &str = "id, seq, act, project_id, project_folder, pattern, actor,
                       recorded_at, ends, content_sha256, prev_hash, chain_hash";

/// A row as SQLite returns it, before the act word is checked.
struct Raw {
    id: String,
    seq: i64,
    act: String,
    project_id: String,
    project_folder: String,
    pattern: Option<String>,
    actor: String,
    recorded_at: String,
    ends: Option<String>,
    content_sha256: String,
    prev_hash: String,
    chain_hash: String,
}

fn row_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Raw> {
    Ok(Raw {
        id: r.get(0)?,
        seq: r.get(1)?,
        act: r.get(2)?,
        project_id: r.get(3)?,
        project_folder: r.get(4)?,
        pattern: r.get(5)?,
        actor: r.get(6)?,
        recorded_at: r.get(7)?,
        ends: r.get(8)?,
        content_sha256: r.get(9)?,
        prev_hash: r.get(10)?,
        chain_hash: r.get(11)?,
    })
}

fn typed(raw: Raw) -> Result<HoldRow> {
    Ok(HoldRow {
        act: Act::parse(&raw.act)?,
        id: raw.id,
        seq: raw.seq,
        project_id: raw.project_id,
        project_folder: raw.project_folder,
        pattern: raw.pattern,
        actor: raw.actor,
        recorded_at: raw.recorded_at,
        ends: raw.ends,
        content_sha256: raw.content_sha256,
        prev_hash: raw.prev_hash,
        chain_hash: raw.chain_hash,
    })
}

/// Every act recorded for this folder, oldest first.
pub fn history(ledger: &Ledger, project_folder: &str) -> Result<Vec<HoldRow>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM capture_holds WHERE project_folder = ?1 ORDER BY seq"
    ))?;
    let raws = stmt
        .query_map([project_folder], row_of)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    raws.into_iter().map(typed).collect()
}

/// Where the record stands for this folder, replayed from its acts in the order
/// they were appended: the pause in force, and every exclusion in force.
pub fn standing(ledger: &Ledger, project_folder: &str) -> Result<Standing> {
    Ok(replay(&history(ledger, project_folder)?))
}

/// [`standing`] from rows already read, oldest first.
fn replay(rows: &[HoldRow]) -> Standing {
    let mut now = Standing::default();
    for row in rows {
        match row.act {
            Act::Paused => now.paused = Some(row.clone()),
            Act::Resumed => now.paused = None,
            Act::Excluded => now.exclusions.push(row.clone()),
            Act::Included => {
                if let Some(ended) = row.ends.as_deref() {
                    now.exclusions.retain(|e| e.id != ended);
                }
            }
        }
    }
    now
}

/// The absolute path as its components, with `.` and a doubled `/` dropped and
/// `..` taken lexically. `None` for a path that is not absolute: a relative
/// `cwd` says nothing about which folder it is in, so it is never "inside".
fn components(path: &str) -> Option<Vec<&str>> {
    if !path.starts_with('/') {
        return None;
    }
    let mut out = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    Some(out)
}

/// `path` is `folder` or lies inside it, **by whole path components**: `/a/b` is
/// not inside `/a/bc`, and `/a/bc` is not inside `/a/b`. Both must be absolute;
/// a relative one is never inside anything.
pub fn path_is_inside(folder: &str, path: &str) -> bool {
    match (components(folder), components(path)) {
        (Some(f), Some(p)) => p.len() >= f.len() && p[..f.len()] == f[..],
        _ => false,
    }
}

/// The path as the disk spells it, when it can say: a symlink, or a `/var` that
/// is really `/private/var`, must not let a recorded pause be walked around. A
/// path that is not on disk is taken as written.
pub fn on_disk(path: &str) -> String {
    std::fs::canonicalize(path)
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| path.to_string())
}

/// Whether `path` is inside `folder`, as written or as the disk spells both.
pub fn folder_covers(folder: &str, path: &str) -> bool {
    path_is_inside(folder, path) || path_is_inside(&on_disk(folder), &on_disk(path))
}

/// Every pause or resume act, oldest first: the few rows every pause question
/// is asked over. Exclusions are not read.
fn pause_rows(ledger: &Ledger) -> Result<Vec<HoldRow>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM capture_holds WHERE act IN ('paused','resumed') ORDER BY seq"
    ))?;
    let raws = stmt
        .query_map([], row_of)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    raws.into_iter().map(typed).collect()
}

/// The pause in force that covers `path` (C34 Q5, the read every capture gate
/// makes): the folder whose latest pause/resume act is a pause, and which
/// contains `path` by whole path components. When folders nest and more than
/// one is paused, the most specific is returned. `None` when nothing in force
/// covers the path. One scan of the few rows the table holds; it writes nothing.
///
/// An `Err` is a record that could not be read. The hook caller treats it as
/// "not paused" (a delivery lost to a failed read is lost for good); callers
/// that can read again later treat it as a refusal.
pub fn standing_for(ledger: &Ledger, path: &str) -> Result<Option<HoldRow>> {
    let mut latest: std::collections::BTreeMap<String, Option<HoldRow>> =
        std::collections::BTreeMap::new();
    for row in pause_rows(ledger)? {
        let folder = row.project_folder.clone();
        match row.act {
            Act::Paused => {
                latest.insert(folder, Some(row));
            }
            _ => {
                latest.insert(folder, None);
            }
        }
    }
    Ok(latest
        .into_values()
        .flatten()
        .filter(|p| folder_covers(&p.project_folder, path))
        .max_by_key(|p| p.project_folder.len()))
}

/// One stretch during which a folder was paused: from the pause's own
/// `recorded_at` up to, and not including, the resumption's. `to` is `None`
/// while the pause stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PauseWindow {
    pub folder: String,
    pub from: String,
    pub to: Option<String>,
    /// The pause row's id, and the resumption's once there is one.
    pub paused_id: String,
    pub resumed_id: Option<String>,
}

/// Every pause window the record holds, oldest first. A window is closed by the
/// resumption that names it in `ends`, not by order or by the clock.
pub fn pause_windows(ledger: &Ledger) -> Result<Vec<PauseWindow>> {
    let mut windows: Vec<PauseWindow> = Vec::new();
    for row in pause_rows(ledger)? {
        match row.act {
            Act::Paused => windows.push(PauseWindow {
                folder: row.project_folder,
                from: row.recorded_at,
                to: None,
                paused_id: row.id,
                resumed_id: None,
            }),
            _ => {
                if let Some(ended) = row.ends.as_deref() {
                    if let Some(w) = windows.iter_mut().find(|w| w.paused_id == ended) {
                        w.to = Some(row.recorded_at.clone());
                        w.resumed_id = Some(row.id.clone());
                    }
                }
            }
        }
    }
    Ok(windows)
}

/// The last pause/resume row's chain hash, or `None` when there is none: what a
/// cache of anything that reads a pause is keyed on, so it never serves bytes
/// from before an act. A store with no pause or resume act has no key, and
/// anything keyed on it is keyed exactly as it was before this existed.
pub fn pause_head(ledger: &Ledger) -> Result<Option<String>> {
    Ok(ledger
        .conn()
        .query_row(
            "SELECT chain_hash FROM capture_holds WHERE act IN ('paused','resumed')
             ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?)
}

/// One path pattern as a person reads it: built-in or typed, on or off, and
/// where that comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExclusionView {
    /// The built-in key (`env`) or the relative path pattern as typed.
    pub key: String,
    /// What it covers: the built-in's words, or the pattern itself.
    pub covers: String,
    /// One short line under a built-in; absent for a typed path.
    pub note: Option<String>,
    pub built_in: bool,
    pub on: bool,
    /// `default` while no row of this folder names the key, `recorded` once one
    /// does.
    pub source: &'static str,
    /// For a default: how it stands without a row, in the words C34 Q7 gives.
    pub basis: Option<&'static str>,
    /// When the current state began: the row that made it so. Absent for a
    /// default.
    pub since: Option<String>,
    /// Who made it so. Absent for a default.
    pub by: Option<String>,
}

/// What a folder's record says, with the built-in defaults laid in: the shape
/// `GET /api/hq/projects/<id>/capture` serves as `holds`, and the terminal
/// prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HoldsView {
    pub paused: bool,
    /// When the pause in force was recorded.
    pub paused_at: Option<String>,
    /// Who paused.
    pub paused_by: Option<String>,
    /// The three built-ins first, in [`BUILT_INS`] order, then every typed path
    /// in force, oldest first. A typed path that has been included again is not
    /// listed.
    pub exclusions: Vec<ExclusionView>,
    /// Whether the six capture callers act on a pause (C34 Q5): `true`.
    pub honoured: bool,
    /// Whether served views and exports apply active path exclusions (C34 Q4).
    pub exclusions_honoured: bool,
    pub note: &'static str,
}

/// Where this folder's record stands, as a person reads it.
pub fn holds_view(ledger: &Ledger, project_folder: &str) -> Result<HoldsView> {
    Ok(view_of(&history(ledger, project_folder)?))
}

fn view_of(rows: &[HoldRow]) -> HoldsView {
    let now = replay(rows);
    let named = |pattern: &str| {
        rows.iter()
            .rev()
            .find(|r| r.pattern.as_deref() == Some(pattern))
    };
    let mut exclusions = Vec::new();
    for b in BUILT_INS {
        let last = named(b.key);
        let on = match last {
            None => b.on_by_default,
            Some(row) => row.act == Act::Excluded,
        };
        exclusions.push(ExclusionView {
            key: b.key.to_string(),
            covers: b.covers.to_string(),
            note: Some(b.note.to_string()),
            built_in: true,
            on,
            source: if last.is_some() {
                "recorded"
            } else {
                "default"
            },
            basis: last.is_none().then_some(if b.on_by_default {
                "on by default (not recorded)"
            } else {
                "off by default (not recorded)"
            }),
            since: last.map(|r| r.recorded_at.clone()),
            by: last.map(|r| r.actor.clone()),
        });
    }
    for row in &now.exclusions {
        let Some(pattern) = row.pattern.as_deref() else {
            continue;
        };
        if built_in(pattern).is_some() {
            continue;
        }
        exclusions.push(ExclusionView {
            key: pattern.to_string(),
            covers: pattern.to_string(),
            note: None,
            built_in: false,
            on: true,
            source: "recorded",
            basis: None,
            since: Some(row.recorded_at.clone()),
            by: Some(row.actor.clone()),
        });
    }
    HoldsView {
        paused: now.paused.is_some(),
        paused_at: now.paused.as_ref().map(|p| p.recorded_at.clone()),
        paused_by: now.paused.as_ref().map(|p| p.actor.clone()),
        exclusions,
        honoured: HONOURED,
        exclusions_honoured: EXCLUSIONS_HONOURED,
        note: HONOURED_NOTE,
    }
}

/// Whether the six capture callers act on a pause: the hook delivery, the
/// speaker audit, the HQ load, the first-use register, the Codex live observer
/// and `thruline ingest` (C34 Q6). They do, and `capture_pause` is where.
pub const HONOURED: bool = true;

/// Served views and exports apply active path exclusions (C34 Q4).
pub const EXCLUSIONS_HONOURED: bool = true;

/// Re-fold the whole chain link by link from the rows' own columns; any
/// mismatch names the seq. Returns how many links were checked.
pub fn verify_chain(ledger: &Ledger) -> Result<u64> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM capture_holds ORDER BY seq"))?;
    let raws = stmt
        .query_map([], row_of)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut prev = genesis_prev_hash(CHAIN_SCOPE);
    let mut checked = 0u64;
    for raw in raws {
        let row = typed(raw)?;
        let expected_seq = checked as i64;
        let hold = Hold {
            project_id: &row.project_id,
            project_folder: &row.project_folder,
            actor: &row.actor,
            recorded_at: &row.recorded_at,
        };
        let sha = sha256_hex(
            content(
                &row.id,
                row.act,
                &hold,
                row.pattern.as_deref(),
                row.ends.as_deref(),
            )?
            .as_bytes(),
        );
        let link = chain_hash(&prev, &sha, CHAIN_SCOPE, row.seq);
        if row.seq != expected_seq
            || sha != row.content_sha256
            || row.prev_hash != prev
            || row.chain_hash != link
        {
            return Err(CoreError::Integrity(format!(
                "capture holds chain does not add up at seq {}",
                row.seq
            )));
        }
        prev = row.chain_hash;
        checked += 1;
    }
    Ok(checked)
}
