//! P6-5B (R-2026-09-23-17) — permission for checks to run in one project
//! folder is a decision, so it is in the record.
//!
//! P6-5 asked Sam for the exact recorded worktree folder and kept the answer
//! in that folder's `.thruline/`. That file is a fast lookup and nothing more: a
//! consent the record does not contain is a decision the product cannot show.
//! So every grant and every revocation is ALSO a row here, appended before the
//! file is written or removed, with who, when, which project and which folder.
//!
//! WHICH WINS. The record. A file can only take permission away, never add it:
//! a run goes ahead only when the file and the latest row here for the same
//! folder agree (the run route's `consent`). A row that is a revocation stops
//! every run whatever the file says; a file that names no standing grant here
//! is not a permission; a standing grant whose file has gone is asked again,
//! because the folder the file lived in may not be the folder that was allowed.
//!
//! THE CHAIN. Machine-wide, scope `run_permission`, the same construction as
//! `speaker_findings`: `content_sha256` is the sha256 of the row's own columns
//! in [`content`]'s canonical encoding, `chain_hash` links it to the row
//! before, and [`verify_chain`] re-folds it from the rows alone. The table's
//! triggers refuse every UPDATE and DELETE and a second row with the same id.
//!
//! Nothing here runs a check, reads a verdict or touches the repository.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// The chain's scope: its genesis is `sha256("run_permission")`.
pub const CHAIN_SCOPE: &str = "run_permission";

/// The table, named once for the screens that cite a row by it.
pub const TABLE: &str = "run_permissions";

/// The two acts, and there is no third.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Act {
    Granted,
    Revoked,
}

impl Act {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Revoked => "revoked",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "granted" => Ok(Self::Granted),
            "revoked" => Ok(Self::Revoked),
            other => Err(CoreError::Integrity(format!(
                "a run permission act must be granted or revoked, not {other:?}"
            ))),
        }
    }
}

/// One row, exactly as the table holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConsentRow {
    pub id: String,
    pub seq: i64,
    pub act: Act,
    pub project_id: String,
    pub project_folder: String,
    pub actor: String,
    pub recorded_at: String,
    /// On a revocation, the grant it takes back.
    pub revokes: Option<String>,
    pub content_sha256: String,
    pub prev_hash: String,
    pub chain_hash: String,
}

/// What one act says, before it has a place on the chain.
#[derive(Debug, Clone)]
pub struct Consent<'a> {
    pub project_id: &'a str,
    pub project_folder: &'a str,
    pub actor: &'a str,
    pub recorded_at: &'a str,
}

/// Where the record stands for one folder: the latest act, or none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Nothing was ever recorded for this folder.
    Never,
    /// The latest act is a grant.
    Granted(ConsentRow),
    /// The latest act took a grant back.
    Revoked(ConsentRow),
}

/// The canonical content a row's `content_sha256` is taken over: its own
/// columns, as a JSON array of strings. JSON string escaping makes the
/// encoding injective — no two different rows share a pre-image, whatever a
/// folder name or an actor holds.
pub fn content(id: &str, act: Act, consent: &Consent<'_>, revokes: Option<&str>) -> Result<String> {
    Ok(serde_json::to_string(&[
        "run_permission_v1",
        id,
        act.as_str(),
        consent.project_id,
        consent.project_folder,
        consent.actor,
        consent.recorded_at,
        revokes.unwrap_or(""),
    ])?)
}

fn require(value: &str, what: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "a run permission must name its {what}"
        )));
    }
    Ok(())
}

/// What a grant came to (ADV-run-ledger-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Granting {
    /// This call appended the grant.
    Appended(ConsentRow),
    /// The folder's latest row was no longer the one the caller's decision
    /// rested on — somebody else acted first — so nothing was appended. This
    /// is where the record stands now.
    Moved(Standing),
}

/// What a row-bound revocation came to. A Settings press only takes back the
/// grant it showed, even if another writer changed the folder meanwhile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Revoking {
    Appended(ConsentRow),
    Moved(Standing),
}

/// The id of the row a standing rests on, or `None` when nothing was ever
/// recorded for the folder.
pub fn latest_id(standing: &Standing) -> Option<&str> {
    match standing {
        Standing::Never => None,
        Standing::Granted(row) | Standing::Revoked(row) => Some(row.id.as_str()),
    }
}

/// Which standing a grant may be appended over. Read inside the append's
/// IMMEDIATE transaction, so no second writer can slip in between the read
/// and the row.
#[derive(Clone, Copy)]
enum Over<'a> {
    /// Anything but a standing grant: a grant over a grant appends nothing.
    NoStandingGrant,
    /// Exactly the row the caller's decision rested on (`None`: nothing yet).
    Latest(Option<&'a str>),
}

/// Append one act on the chain, in one IMMEDIATE transaction so two writers
/// cannot take the same place. Both acts read the folder's standing inside the
/// same transaction: a revocation so two presses cannot both take back one
/// grant (with no standing grant nothing is appended), and a grant so that
/// racing presses of "Allow" append one grant, not one each
/// (ADV-run-ledger-4). When nothing is appended, `Err` carries where the
/// record stands instead.
fn append(
    ledger: &Ledger,
    act: Act,
    consent: &Consent<'_>,
    over: Over<'_>,
) -> Result<std::result::Result<ConsentRow, Standing>> {
    for (value, what) in [
        (consent.project_id, "project"),
        (consent.project_folder, "project folder"),
        (consent.actor, "actor"),
        (consent.recorded_at, "time"),
    ] {
        require(value, what)?;
    }
    let conn = ledger.conn();
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<std::result::Result<ConsentRow, Standing>> {
        let now = standing(ledger, consent.project_folder)?;
        let revokes = match (act, over) {
            (Act::Granted, Over::NoStandingGrant) => match now {
                Standing::Granted(_) => return Ok(Err(now)),
                Standing::Never | Standing::Revoked(_) => None,
            },
            (Act::Granted, Over::Latest(seen)) => {
                if latest_id(&now) != seen {
                    return Ok(Err(now));
                }
                None
            }
            (Act::Revoked, Over::Latest(seen)) if latest_id(&now) != seen => {
                return Ok(Err(now));
            }
            (Act::Revoked, _) => match now {
                Standing::Granted(grant) => Some(grant.id),
                Standing::Never | Standing::Revoked(_) => return Ok(Err(now)),
            },
        };
        let revokes = revokes.as_deref();
        let head: Option<(String, i64)> = conn
            .query_row(
                "SELECT chain_hash, seq FROM run_permissions ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (prev_hash, seq) = match head {
            Some((hash, last)) => (hash, last + 1),
            None => (genesis_prev_hash(CHAIN_SCOPE), 0),
        };
        let id = format!("rpm_{}", ulid::Ulid::generate());
        let content_sha256 = sha256_hex(content(&id, act, consent, revokes)?.as_bytes());
        let chain = chain_hash(&prev_hash, &content_sha256, CHAIN_SCOPE, seq);
        conn.execute(
            "INSERT INTO run_permissions(
                 id, seq, act, project_id, project_folder, actor,
                 recorded_at, revokes, content_sha256, prev_hash, chain_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                id,
                seq,
                act.as_str(),
                consent.project_id,
                consent.project_folder,
                consent.actor,
                consent.recorded_at,
                revokes,
                content_sha256,
                prev_hash,
                chain,
            ],
        )?;
        Ok(Ok(ConsentRow {
            id,
            seq,
            act,
            project_id: consent.project_id.to_string(),
            project_folder: consent.project_folder.to_string(),
            actor: consent.actor.to_string(),
            recorded_at: consent.recorded_at.to_string(),
            revokes: revokes.map(str::to_string),
            content_sha256,
            prev_hash,
            chain_hash: chain,
        }))
    })();
    match outcome {
        Ok(row) => {
            conn.execute_batch("COMMIT")?;
            Ok(row)
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// Record that a person allowed checks to run in this folder.
///
/// Over a grant that already stands nothing is appended and that grant comes
/// back: the standing is read inside the append's transaction, so two callers
/// racing from "never" append one grant between them (ADV-run-ledger-4). A
/// caller that must grant again over a standing grant — the run route, when
/// the grant's copy in the folder has gone and the person was asked again —
/// says which row its decision rested on, through [`grant_over`].
pub fn grant(ledger: &Ledger, consent: &Consent<'_>) -> Result<ConsentRow> {
    match append(ledger, Act::Granted, consent, Over::NoStandingGrant)? {
        Ok(row) | Err(Standing::Granted(row)) => Ok(row),
        Err(_) => Err(CoreError::Integrity("a grant was not appended".to_string())),
    }
}

/// Record that a person allowed checks to run in this folder — over the
/// standing their decision rested on (ADV-run-ledger-4).
///
/// `seen` is the id of the folder's latest row when the person was asked
/// ([`latest_id`] of the [`standing`] read then; `None` for "never"). The
/// grant is appended only if that is still the latest row, read inside the
/// same IMMEDIATE transaction as the append; otherwise nothing is appended and
/// [`Granting::Moved`] says where the record stands now. So ten presses of
/// "Allow" that all saw "never" append one grant, and a grant over a standing
/// grant is appended only when that grant is what the person was shown.
pub fn grant_over(ledger: &Ledger, consent: &Consent<'_>, seen: Option<&str>) -> Result<Granting> {
    Ok(
        match append(ledger, Act::Granted, consent, Over::Latest(seen))? {
            Ok(row) => Granting::Appended(row),
            Err(now) => Granting::Moved(now),
        },
    )
}

/// Record that a person took back the standing grant for this folder.
///
/// `None` when there is no standing grant to take back — nothing is appended,
/// because a revocation must point at the grant it revokes.
pub fn revoke(ledger: &Ledger, consent: &Consent<'_>) -> Result<Option<ConsentRow>> {
    Ok(append(ledger, Act::Revoked, consent, Over::NoStandingGrant)?.ok())
}

/// Take back exactly the standing grant the person saw. The comparison and
/// append share one IMMEDIATE transaction, so a later grant cannot be revoked
/// by a stale Settings press from another process.
pub fn revoke_over(ledger: &Ledger, consent: &Consent<'_>, seen: Option<&str>) -> Result<Revoking> {
    Ok(
        match append(ledger, Act::Revoked, consent, Over::Latest(seen))? {
            Ok(row) => Revoking::Appended(row),
            Err(now) => Revoking::Moved(now),
        },
    )
}

const COLUMNS: &str = "id, seq, act, project_id, project_folder, actor, recorded_at,
                       revokes, content_sha256, prev_hash, chain_hash";

fn row_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        [r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?],
        r.get(7)?,
        [r.get(8)?, r.get(9)?, r.get(10)?],
    ))
}

type RawRow = (
    String,
    i64,
    String,
    [String; 4],
    Option<String>,
    [String; 3],
);

fn typed(raw: RawRow) -> Result<ConsentRow> {
    let (
        id,
        seq,
        act,
        [project_id, project_folder, actor, recorded_at],
        revokes,
        [content_sha256, prev_hash, chain_hash],
    ) = raw;
    Ok(ConsentRow {
        id,
        seq,
        act: Act::parse(&act)?,
        project_id,
        project_folder,
        actor,
        recorded_at,
        revokes,
        content_sha256,
        prev_hash,
        chain_hash,
    })
}

/// Every act recorded for this folder, oldest first.
pub fn history(ledger: &Ledger, project_folder: &str) -> Result<Vec<ConsentRow>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM run_permissions WHERE project_folder = ?1 ORDER BY seq"
    ))?;
    let raws = stmt
        .query_map([project_folder], row_of)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    raws.into_iter().map(typed).collect()
}

/// Where the record stands for this folder: its latest act.
pub fn standing(ledger: &Ledger, project_folder: &str) -> Result<Standing> {
    let raw = ledger
        .conn()
        .query_row(
            &format!(
                "SELECT {COLUMNS} FROM run_permissions WHERE project_folder = ?1
                  ORDER BY seq DESC LIMIT 1"
            ),
            [project_folder],
            row_of,
        )
        .optional()?;
    Ok(match raw.map(typed).transpose()? {
        None => Standing::Never,
        Some(row) if row.act == Act::Granted => Standing::Granted(row),
        Some(row) => Standing::Revoked(row),
    })
}

/// Re-fold the whole chain link by link from the rows' own columns; any
/// mismatch names the seq. Returns how many links were checked.
pub fn verify_chain(ledger: &Ledger) -> Result<u64> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM run_permissions ORDER BY seq"
    ))?;
    let raws = stmt
        .query_map([], row_of)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut prev = genesis_prev_hash(CHAIN_SCOPE);
    let mut checked = 0u64;
    for raw in raws {
        let row = typed(raw)?;
        let expected_seq = checked as i64;
        let consent = Consent {
            project_id: &row.project_id,
            project_folder: &row.project_folder,
            actor: &row.actor,
            recorded_at: &row.recorded_at,
        };
        let sha =
            sha256_hex(content(&row.id, row.act, &consent, row.revokes.as_deref())?.as_bytes());
        let link = chain_hash(&prev, &sha, CHAIN_SCOPE, row.seq);
        if row.seq != expected_seq
            || sha != row.content_sha256
            || row.prev_hash != prev
            || row.chain_hash != link
        {
            return Err(CoreError::Integrity(format!(
                "run permissions chain does not add up at seq {}",
                row.seq
            )));
        }
        prev = row.chain_hash;
        checked += 1;
    }
    Ok(checked)
}
