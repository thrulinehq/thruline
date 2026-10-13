//! F1 positive witnesses from accepted assured captures and verified raw calls.
//! A missing project-wide census can never produce Holds. One exact witness
//! can prove a forbidden invocation without asserting coverage of other calls.

use std::path::Path;
use std::process::Command;

use rusqlite::{params, OptionalExtension};

use crate::capture::{self, raw_tool_name, registered_command};
use crate::chain::sha256_hex;
use crate::error::Result;
use crate::ledger::Ledger;
use crate::redact::Masked;

use super::capture_tree_basis;
use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::checks_process::CommandForbiddenParams;
use super::params::Surface;
use super::record_source;

/// Returns Broken only for a re-verified agent invocation and assured stable
/// capture whose recorded HEAD names the selected commit/tree and whose
/// recorded content/index digests match that tree. Historical filesystem
/// executable bits are not observed; this is an invocation finding, not a
/// full historical cleanliness assertion. All other outcomes abstain.
pub fn check_forbidden_at(
    ledger: &Ledger,
    repo: &Path,
    project_id: &str,
    version_id: &str,
    tree_hash: &str,
    rule: &CommandForbiddenParams,
) -> Result<CheckOutcome> {
    if !exact_commit_tree(repo, version_id, tree_hash) {
        return Ok(unavailable(
            "named commit and tree cannot be verified",
            tree_hash,
        ));
    }
    let mut content_basis = None;
    let mut projects = Vec::new();
    if let Some(project) = crate::hq::find_project(ledger, project_id)? {
        if !same_repository(Path::new(&project.root), repo)? {
            return Ok(unavailable(
                "recorded project root differs from the selected repository",
                tree_hash,
            ));
        }
        projects.extend(
            project
                .members
                .into_iter()
                .map(|member| member.ledger_project_id),
        );
    } else {
        // A direct ledger project is allowed only when its recorded root is this repository.
        let root: Option<String> = ledger
            .conn()
            .query_row(
                "SELECT root_path FROM projects WHERE id=?1",
                [project_id],
                |row| row.get(0),
            )
            .optional()?;
        if !root
            .as_deref()
            .is_some_and(|root| same_repository(Path::new(root), repo).unwrap_or(false))
        {
            return Ok(unavailable(
                "recorded project membership cannot be verified",
                tree_hash,
            ));
        }
        projects.push(project_id.to_owned());
    }
    let mut witnesses = Vec::new();
    for project in projects {
        for (event_id, attested) in capture::attested_runs_for_project(ledger, &project)? {
            if !attested.assured
                || !attested.accepted
                || !attested.fingerprint_stable
                || attested.head_commit != version_id
            {
                continue;
            }
            let Some(source) = record_source::verified_event(ledger, &project, &event_id)? else {
                continue;
            };
            if source.kind != "tool_use"
                || !matches!(source.role.as_deref(), None | Some("assistant"))
                || rule
                    .session_scope
                    .as_deref()
                    .is_some_and(|scope| scope != source.session_id)
            {
                continue;
            }
            let Some(input) = source.tool_input.as_deref() else {
                continue;
            };
            let Some(command) = registered_command(
                &source.source,
                raw_tool_name(
                    source.tool_name.as_deref().unwrap_or(""),
                    source.source_tool_name.as_deref(),
                ),
                input,
            ) else {
                continue;
            };
            if sha256_hex(command.as_bytes()) != attested.command_sha256 {
                continue;
            }
            let Some(turn) = source.source_turn_id.as_deref() else {
                continue;
            };
            // Exactly one accepted attempt can bind this event; an identical
            // command issued twice in the same turn is not an attribution.
            let mut statement = ledger.conn().prepare(
                "SELECT a.attempt_id,pre.capsule_id,post.capsule_id,
                        pre.head_commit,pre.head_tree,post.head_commit,post.head_tree,pre.complete,post.complete,
                        pre.index_digest,post.index_digest,pre.tracked_worktree_digest,post.tracked_worktree_digest,
                        pre.staged_digest,post.staged_digest,pre.untracked_digest,post.untracked_digest
                 FROM capture_attempts a JOIN source_sessions ss
                   ON ss.source_session_key=a.source_session_key AND ss.owner_key=a.owner_key AND ss.source=a.source
                 JOIN state_capsules pre ON pre.attempt_id=a.attempt_id AND pre.phase='pre'
                 JOIN state_capsules post ON post.attempt_id=a.attempt_id AND post.phase='post'
                 WHERE ss.project_id=?1 AND ss.session_id=?2 AND a.source_turn_id=?3
                   AND a.command_sha256=?4 AND a.strength='assured' AND a.status='accepted'",
            )?;
            let captures = statement
                .query_map(
                    params![project, source.session_id, turn, attested.command_sha256],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, Option<String>>(9)?,
                            row.get::<_, Option<String>>(10)?,
                            row.get::<_, Option<String>>(11)?,
                            row.get::<_, Option<String>>(12)?,
                            row.get::<_, Option<String>>(13)?,
                            row.get::<_, Option<String>>(14)?,
                            row.get::<_, Option<String>>(15)?,
                            row.get::<_, Option<String>>(16)?,
                        ))
                    },
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if captures.len() != 1 {
                continue;
            }
            if literal_invocation_matches(&command, &rule.patterns) != Some(true) {
                continue;
            }
            if content_basis.is_none() {
                content_basis = capture_tree_basis::from_tree(repo, tree_hash);
            }
            let Some(recorded) = content_basis.as_ref() else {
                return Ok(unavailable(
                    "recorded content/index digests cannot be reconstructed from this tree",
                    tree_hash,
                ));
            };
            let capture = &captures[0];
            if capture.3.as_deref() != Some(version_id)
                || capture.4.as_deref() != Some(tree_hash)
                || capture.5.as_deref() != Some(version_id)
                || capture.6.as_deref() != Some(tree_hash)
                || capture.7 != 1
                || capture.8 != 1
                || capture.9.as_deref() != Some(recorded.index.as_str())
                || capture.10.as_deref() != Some(recorded.index.as_str())
                || capture.11.as_deref() != Some(recorded.tracked.as_str())
                || capture.12.as_deref() != Some(recorded.tracked.as_str())
                || capture.13.as_deref() != Some(recorded.staged.as_str())
                || capture.14.as_deref() != Some(recorded.staged.as_str())
                || capture.15.as_deref() != Some(recorded.untracked.as_str())
                || capture.16.as_deref() != Some(recorded.untracked.as_str())
            {
                continue;
            }
            let mut outcome = CheckOutcome {
                state: VerdictState::Broken,
                tree_hash: Some(tree_hash.into()),
                reason: None,
                checked_places: 1,
                findings: vec![Finding {
                    surface: Surface::Other,
                    path: String::new(),
                    line: 0,
                    span: String::new(),
                }],
                surfaces: vec![SurfaceResult {
                    surface: Surface::Other,
                    checked_places: 1,
                    finding_count: 1,
                    state: VerdictState::Broken,
                }],
                not_looked_at: Vec::new(),
                visual: Vec::new(),
            };
            for finding in &mut outcome.findings {
                finding.path = Masked::mask(&source.raw_file).into_text();
                finding.line = source.raw_line as u64;
                finding.span = Masked::mask(&format!(
                    "{} [event {}; raw_sha256 {}; chain_hash {}; attempt {}; pre {}; post {}]",
                    command,
                    source.id,
                    source.raw_sha256,
                    source.chain_hash,
                    capture.0,
                    capture.1,
                    capture.2,
                ))
                .into_text();
            }
            witnesses.push(outcome);
        }
    }
    if witnesses.is_empty() {
        return Ok(unavailable(
            "no verified forbidden invocation; complete command census is not recorded",
            tree_hash,
        ));
    }
    let mut outcome = witnesses.remove(0);
    for witness in witnesses {
        outcome.checked_places += witness.checked_places;
        outcome.findings.extend(witness.findings);
    }
    outcome.reason = Some("verified forbidden invocation with captured HEAD and matching recorded content/index on the selected tree; other command coverage remains incomplete; historical filesystem executable bits are not recorded".into());
    for surface in &mut outcome.surfaces {
        surface.checked_places = outcome.checked_places;
        surface.finding_count = outcome.findings.len() as u64;
    }
    Ok(outcome)
}

fn unavailable(reason: &str, tree_hash: &str) -> CheckOutcome {
    CheckOutcome::unavailable(reason, Some(tree_hash.into()))
}

fn exact_commit_tree(repo: &Path, commit: &str, tree: &str) -> bool {
    if ![commit, tree]
        .iter()
        .all(|oid| matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return false;
    }
    let commit_output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{commit}^{{commit}}"),
        ])
        .current_dir(repo)
        .output();
    if !commit_output.is_ok_and(|output| {
        output.status.success()
            && std::str::from_utf8(&output.stdout).is_ok_and(|oid| oid.trim() == commit)
    }) {
        return false;
    }
    let tree_output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{commit}^{{tree}}"),
        ])
        .current_dir(repo)
        .output();
    tree_output.is_ok_and(|output| {
        output.status.success()
            && std::str::from_utf8(&output.stdout).is_ok_and(|oid| oid.trim() == tree)
    })
}

fn same_repository(recorded: &Path, selected: &Path) -> Result<bool> {
    let recorded_path = recorded.canonicalize().ok();
    let selected_path = selected.canonicalize().ok();
    if recorded_path.is_some() && recorded_path == selected_path {
        return Ok(true);
    }
    if recorded_path.is_none() || selected_path.is_none() {
        return Ok(false);
    }
    let recorded_id = crate::workspace::repo_identity_at(recorded)?.repo_id;
    let selected_id = crate::workspace::repo_identity_at(selected)?.repo_id;
    Ok(recorded_id.is_some() && recorded_id == selected_id)
}

/// A closed direct-invocation subset. Compound shell control flow, expansion,
/// redirection, wrappers and a pattern consisting only of flags are unknown.
/// The shared parser handles quotes, escapes and comments; arguments to echo
/// never become a second program invocation.
fn literal_invocation_matches(command: &str, patterns: &[String]) -> Option<bool> {
    fn argv(text: &str) -> Option<Vec<String>> {
        // Conservative even inside quoted arguments: no compound script gets
        // interpreted as though every syntactic branch actually ran.
        if text
            .chars()
            .any(|ch| matches!(ch, ';' | '&' | '|' | '\n' | '~'))
        {
            return None;
        }
        let mut commands = thruline_verify::shell::split_commands(text).ok()?;
        if commands.len() != 1 {
            return None;
        }
        let parsed = commands.remove(0);
        if parsed.redirected || !parsed.env_prefix.is_empty() || parsed.argv.is_empty() {
            return None;
        }
        if matches!(
            parsed.argv[0].as_str(),
            "if" | "then"
                | "else"
                | "elif"
                | "fi"
                | "for"
                | "while"
                | "until"
                | "case"
                | "esac"
                | "do"
                | "done"
                | "select"
                | "function"
                | "time"
                | "!"
                | "eval"
                | "source"
                | "."
                | "command"
                | "builtin"
                | "exec"
                | "sh"
                | "bash"
                | "zsh"
                | "dash"
                | "ksh"
                | "fish"
        ) {
            return None;
        }
        Some(parsed.argv)
    }
    let actual = argv(command)?;
    if patterns.is_empty() {
        return None;
    }
    let mut hit = false;
    for pattern in patterns {
        let expected = argv(pattern)?;
        if expected[0].starts_with('-') {
            return None;
        }
        hit |= actual.starts_with(&expected);
    }
    Some(hit)
}
