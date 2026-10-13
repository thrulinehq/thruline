//! One bounded advisory line from a stored verdict for the clean HEAD tree.
//!
//! This reader does not open the writable ledger or invoke a check. A missing
//! identity, a dirty worktree, or a busy/unreadable cache is silence.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::decisions::authority::{bridge, PersonActKind};
use crate::decisions::engine::CHECK_IMPL_VERSION;
use crate::redact::Masked;

/// The product's budget for the whole read: two Git spawns, one read-only
/// ledger open and the hash-chain check. Expiry is silence, never an error.
pub const DEFAULT_BUDGET: Duration = Duration::from_millis(50);

/// Return at most one cached broken decision for the proved current commit.
pub fn cached_line(db: &Path, repo: &Path) -> Option<String> {
    cached_line_within(db, repo, DEFAULT_BUDGET)
}

/// [`cached_line`] with the caller's budget. The Stop hook passes
/// [`DEFAULT_BUDGET`] unless its environment *lengthens* it: a machine busy
/// enough to need more than 50 ms for two Git spawns makes the line vanish,
/// which is right inside a session and wrong in a test that asserts the line.
pub fn cached_line_within(db: &Path, repo: &Path, budget: Duration) -> Option<String> {
    let deadline = Instant::now() + budget;
    let cwd = repo.canonicalize().ok()?;
    let identity = git(
        &cwd,
        &[
            "rev-parse",
            "--show-toplevel",
            "HEAD^{commit}",
            "HEAD^{tree}",
        ],
        deadline,
    )?;
    let mut identity = identity.lines();
    let top = identity.next()?;
    let version = identity.next()?.to_owned();
    let tree = identity.next()?.to_owned();
    if identity.next().is_some() || !git_oid(&version) || !git_oid(&tree) {
        return None;
    }
    let repo = Path::new(top).canonicalize().ok()?;
    if !cwd.starts_with(&repo) {
        return None;
    }
    if !git(
        &repo,
        &["status", "--porcelain", "--untracked-files=all"],
        deadline,
    )?
    .is_empty()
    {
        return None;
    }
    let connection: Connection =
        crate::store_hold::open_connection(db, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    connection
        .busy_timeout(std::time::Duration::from_millis(5))
        .ok()?;
    let mut projects = connection
        .prepare("SELECT id,root_path FROM projects ORDER BY id")
        .ok()?;
    let roots = projects
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok()?;
    let mut ids = Vec::new();
    for root in roots {
        let (id, path) = root.ok()?;
        if Path::new(&path).canonicalize().ok().as_deref() == Some(repo.as_path()) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return None;
    }
    let mut statement = connection
        .prepare(
            "SELECT d.id,r.text,a.id,a.rev
             FROM decisions d
             JOIN decision_acceptances a ON a.decision_id=d.id
             JOIN decision_revisions r ON r.decision_id=d.id AND r.rev=a.rev
             WHERE d.project_id=?1
             ORDER BY d.id,a.rowid DESC",
        )
        .ok()?;
    for project_id in ids {
        let mut rows = statement.query([project_id]).ok()?;
        let mut selected_decision: Option<String> = None;
        while let Some(row) = rows.next().ok()? {
            let id: String = row.get(0).ok()?;
            if selected_decision.as_deref() == Some(&id) {
                continue;
            }
            let acceptance_id: String = row.get(2).ok()?;
            let revision: u32 = row.get(3).ok()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            if !bridge::verified_read_only(
                &connection,
                PersonActKind::Accept,
                &acceptance_id,
                remaining,
            )
            .ok()?
            {
                continue;
            }
            // A pending newer act cannot displace this verified acceptance,
            // but an older accepted revision cannot supply its verdict.
            selected_decision = Some(id.clone());
            let mut retired = false;
            let mut retirement_rows = connection
                .prepare(
                    "SELECT id FROM decision_retirements WHERE decision_id=?1 ORDER BY rowid DESC",
                )
                .ok()?;
            let retirement_ids = retirement_rows
                .query_map([&id], |row| row.get::<_, String>(0))
                .ok()?
                .collect::<rusqlite::Result<Vec<_>>>()
                .ok()?;
            for retirement_id in retirement_ids {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return None;
                }
                if bridge::verified_read_only(
                    &connection,
                    PersonActKind::Retire,
                    &retirement_id,
                    remaining,
                )
                .ok()?
                {
                    retired = true;
                    break;
                }
            }
            if retired {
                continue;
            }
            if Instant::now() >= deadline {
                return None;
            }
            let verdict: Option<(String, String)> = connection
                .query_row(
                    "SELECT verdict,findings_json FROM decision_verdicts
                     WHERE decision_id=?1 AND rev=?2 AND version_id=?3 AND tree_hash=?4
                       AND check_impl_version=?5
                     ORDER BY rowid DESC LIMIT 1",
                    params![id, revision, version, tree, CHECK_IMPL_VERSION],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .ok()?;
            let Some((state, findings)) = verdict else {
                continue;
            };
            if state != "broken" {
                continue;
            }
            let span: String = row.get(1).ok()?;
            let span = bounded_span(&span);
            let count = serde_json::from_str::<serde_json::Value>(&findings)
                .ok()
                .and_then(|value| value.as_array().map(Vec::len))
                .filter(|count| *count > 0);
            let places = count.map_or_else(String::new, |n| {
                format!(" ({n} {})", if n == 1 { "place" } else { "places" })
            });
            return Some(format!(
                "Decision broken on {}: \"{span}\"{places} · why {id}",
                &version[..8]
            ));
        }
    }
    None
}

fn git(repo: &Path, args: &[&str], deadline: Instant) -> Option<String> {
    bounded_command_output(
        crate::git_isolation::isolate_config(&mut Command::new("git"))
            .args(["-c", "core.fsmonitor=false", "-C"])
            .arg(repo)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0"),
        deadline,
    )
}

fn kill_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn bounded_command_output(command: &mut Command, deadline: Instant) -> Option<String> {
    const MAX_OUTPUT: u64 = 256 * 1024;
    if Instant::now() >= deadline {
        return None;
    }
    // A file-backed stdout prevents a large `git status` from blocking in a
    // full pipe while this thread is watching the hard deadline.
    let mut output = tempfile::tempfile().ok()?;
    let child_output = output.try_clone().ok()?;
    let mut child = command
        .stdout(Stdio::from(child_output))
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    loop {
        if Instant::now() >= deadline {
            kill_reap(&mut child);
            return None;
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
            Ok(None) => thread::sleep(Duration::from_millis(1)),
            Err(_) => {
                kill_reap(&mut child);
                return None;
            }
        }
    }
    if Instant::now() >= deadline || output.seek(SeekFrom::Start(0)).is_err() {
        return None;
    }
    let mut bytes = Vec::new();
    output.take(MAX_OUTPUT + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_OUTPUT || Instant::now() >= deadline {
        return None;
    }
    String::from_utf8(bytes).ok().map(|text| text.trim().into())
}

fn git_oid(text: &str) -> bool {
    (text.len() == 40 || text.len() == 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn bounded_span(text: &str) -> String {
    let masked = Masked::mask(text).into_text();
    let compact = masked.split_whitespace().collect::<Vec<_>>().join(" ");
    let compact = compact.trim_matches('"');
    if compact.is_empty() {
        return "accepted decision".into();
    }
    let mut chars = compact.chars();
    let short: String = chars.by_ref().take(96).collect();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_kills_and_reaps_a_slow_child_before_the_outer_speaker_budget() {
        let mut child = Command::new("/bin/sleep");
        child.arg("2");
        let start = Instant::now();
        assert!(bounded_command_output(&mut child, start + Duration::from_millis(30)).is_none());
        assert!(start.elapsed() < Duration::from_millis(150));
    }
}
