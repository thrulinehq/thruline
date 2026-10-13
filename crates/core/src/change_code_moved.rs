//! STEP-7-GIT-LINE (R-2026-09-28-42; founder ruling #21 (a): "The read-time
//! git line, cautious direction only, with its receipt shown.")
//!
//! After the agent commits, the version a Change shows is the code as ThruLine
//! last RECORDED it — nothing re-records it when a page opens. This module
//! reads git at that moment and answers one question: is the folder's HEAD a
//! different commit from the one recorded with the version?
//!
//! **Cautious direction only.** It answers `Some` only when the two commits
//! differ AND git answered every read within [`READ_LIMIT`]. Everything else —
//! equal heads, a recorded folder that is gone or is no longer a repository, a
//! recorded commit git does not know, a git that does not answer — is `None`,
//! and `None` means "nothing said", never "unchanged". Uncommitted edits are
//! not compared at all, and the answer says so in `uncommitted_compared`.
//!
//! **Read-only, structurally.** The only git verbs are `rev-parse`,
//! `rev-list --count` and `diff --name-only`, each through
//! [`crate::bounded_child::output_until`] (the existing bounded child: a group
//! of its own, killed and reaped at the deadline). It writes no row, and it
//! never calls `bind_intended_at`, `on_intended_bound` or `assess_tree_move`:
//! re-binding the version on read would let linked evidence read "Still
//! applies" on the agent's code, which is claiming more.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use rusqlite::params;
use serde::Serialize;

use crate::error::Result;
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// How long the whole comparison may take — every git read shares this one
/// deadline. A page is waiting on it, so it is shorter than the capture
/// bound; a git that has not answered by then has said nothing.
pub const READ_LIMIT: Duration = Duration::from_secs(5);

/// What moved, with its receipt: the commit recorded with the version and the
/// commit the recorded folder is at now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeMoved {
    /// The HEAD recorded on the Change's standing `intended` row.
    pub recorded: String,
    /// The HEAD of the recorded folder when this was read.
    pub now: String,
    /// Repository-relative paths that differ between the two commits,
    /// without the product's own `.thruline/` folder, masked like every other
    /// served text.
    pub changed_paths: Vec<String>,
    /// Commits reachable from `now` and not from `recorded`.
    pub commits: u64,
    /// Always `false` in this slice: edits not yet committed are not
    /// compared, and silence about them is never "unchanged".
    pub uncommitted_compared: bool,
}

/// The git program and the bound. [`GitReader::system`] is the product's; the
/// tests put a stand-in in front, the same seam as `snapshot::Tools`.
#[derive(Debug, Clone)]
pub struct GitReader {
    pub program: OsString,
    pub limit: Duration,
}

impl GitReader {
    pub fn system() -> GitReader {
        GitReader {
            program: OsString::from("git"),
            limit: READ_LIMIT,
        }
    }
}

/// [`code_moved_with`] through the system's `git` and [`READ_LIMIT`].
pub fn code_moved(ledger: &Ledger, change_id: &str) -> Result<Option<CodeMoved>> {
    code_moved_with(ledger, change_id, &GitReader::system())
}

/// Compare the recorded HEAD with the recorded folder's HEAD now.
pub fn code_moved_with(
    ledger: &Ledger,
    change_id: &str,
    git: &GitReader,
) -> Result<Option<CodeMoved>> {
    Ok(read(ledger, change_id, git)?.map(|(moved, _)| moved))
}

/// The same answer, with what the masker did to its paths, for the one
/// caller that serves it beside the rest of a Change's masked text.
pub(crate) fn read(
    ledger: &Ledger,
    change_id: &str,
    git: &GitReader,
) -> Result<Option<(CodeMoved, MaskRecord)>> {
    /* A version bound on purpose (`change_implementations`) is not the
     * `intended` row this compares against, and a sentence naming one version
     * while measuring from another would be wrong in the confident direction.
     * Nothing this product ships writes that table; where something has,
     * this says nothing. */
    let bound: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM change_implementations WHERE change_id=?1",
        params![change_id],
        |row| row.get(0),
    )?;
    if bound > 0 {
        return Ok(None);
    }
    let Some(intended) = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?
    else {
        return Ok(None);
    };
    let Some(recorded) = intended.base_revision.value().map(str::to_string) else {
        return Ok(None);
    };
    if !is_commit_name(&recorded) {
        return Ok(None);
    }
    let Some(root) = crate::change_decision::worktree_root(ledger, change_id)? else {
        return Ok(None);
    };
    if !root.is_dir() {
        return Ok(None);
    }
    Ok(compare(&root, &recorded, git))
}

/// Hex only, so a stored value can never be read by git as an option.
fn is_commit_name(value: &str) -> bool {
    (4..=64).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn compare(root: &Path, recorded: &str, git: &GitReader) -> Option<(CodeMoved, MaskRecord)> {
    let deadline = Instant::now() + git.limit;
    let recorded_full = answer(
        git,
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{recorded}^{{commit}}"),
        ],
        deadline,
    )?
    .trim()
    .to_string();
    let now = answer(
        git,
        root,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
        deadline,
    )?
    .trim()
    .to_string();
    if recorded_full.is_empty() || now.is_empty() || recorded_full == now {
        return None;
    }
    let commits: u64 = answer(
        git,
        root,
        &["rev-list", "--count", &format!("{recorded_full}..{now}")],
        deadline,
    )?
    .trim()
    .parse()
    .ok()?;
    let names = answer(
        git,
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            &recorded_full,
            &now,
        ],
        deadline,
    )?;
    let mut record = MaskRecord::default();
    let changed_paths = names
        .split('\0')
        .filter(|p| !p.is_empty() && !crate::change_decision::is_our_own(p))
        /* The Change's own masker (a classified call site), so the paths
         * are treated exactly like every other served text of a Change. */
        .map(|p| crate::change::mask_text(p, &mut record))
        .collect();
    Some((
        CodeMoved {
            recorded: recorded_full,
            now,
            changed_paths,
            commits,
            uncommitted_compared: false,
        },
        record,
    ))
}

/// One git read inside the shared deadline, its stdout as written. Anything
/// but a finished, successful child is `None`: a partial or failed read is
/// not a read.
fn answer(git: &GitReader, root: &Path, args: &[&str], deadline: Instant) -> Option<String> {
    let mut command = Command::new(&git.program);
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(root)
        .args(args);
    match crate::bounded_child::output_until(&mut command, deadline) {
        Ok(crate::bounded_child::Outcome::Finished(out)) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        _ => None,
    }
}
