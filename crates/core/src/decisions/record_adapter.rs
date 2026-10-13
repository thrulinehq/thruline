//! Read existing captured evidence for D1, D2 and E1.
//!
//! The command wrapper records a complete execution receipt and an input
//! identity keyed to its binding. Edit events record paths but have no
//! complete, exact-version path and actor census, so D1 and D2 abstain here.

use std::path::Path;
use std::process::Command;

use crate::error::Result;
use crate::evidence_binding::{self, EvidenceState, ScopeClass, WsidScopeDecl};
use crate::examined_inputs::{self, SubjectKind, Uncommitted};
use crate::ledger::Ledger;
use crate::run_execution;
use crate::test_tally;
use crate::workspace;

use super::checks::CheckOutcome;
use super::checks_record::{check_command_passes, RecordedCommandRun, RunCensus};
use super::params::CheckParams;

/// Return `Some` for kinds whose evidence comes from the append-only record.
/// A missing evidence link is an explicit unverifiable outcome, never a hold.
pub fn check_recorded_at(
    ledger: &Ledger,
    repo: &Path,
    version_id: &str,
    tree_hash: &str,
    params: &CheckParams,
) -> Result<Option<CheckOutcome>> {
    match params {
        CheckParams::EditsWithinPaths(_) | CheckParams::PathsUntouched(_) => {
            Ok(Some(CheckOutcome::unavailable(
                "complete exact-version edit census is not recorded",
                Some(tree_hash.into()),
            )))
        }
        CheckParams::CommandPasses(command) => {
            let Some(commit) = exact_commit(repo, version_id, tree_hash) else {
                return Ok(Some(CheckOutcome::unavailable(
                    "named version cannot be tied to the recorded tree",
                    Some(tree_hash.into()),
                )));
            };
            let Some(repo_id) = workspace::repo_identity_at(repo)?.repo_id else {
                return Ok(Some(CheckOutcome::unavailable(
                    "repository identity is unavailable",
                    Some(tree_hash.into()),
                )));
            };
            let mut runs = Vec::new();
            for stored in evidence_binding::all_for_repo(ledger, &repo_id)? {
                let binding = &stored.binding;
                if evidence_binding::rendered_command(&binding.argv) != command.command.trim() {
                    continue;
                }
                let Some(inputs) =
                    examined_inputs::standing(ledger, SubjectKind::CheckRun, &stored.id)?
                else {
                    continue;
                };
                if inputs.base_revision.value() != Some(commit.as_str()) {
                    continue;
                }
                let execution = run_execution::latest(ledger, &binding.source_session)?;
                let complete = matches!(inputs.uncommitted, Uncommitted::Clean)
                    && inputs.source_identity.value().is_some()
                    && inputs.source_identity.value() == binding.pre.identity.as_deref()
                    && inputs.source_identity.value() == binding.post.identity.as_deref()
                    && binding.wsid_scope == WsidScopeDecl::everything()
                    && evidence_binding::state_at_binding(binding)
                        == EvidenceState::CurrentStateConfirmed
                    && binding.term_signal.is_none()
                    && binding.wrapper_defects.is_empty()
                    && (!command.is_test || binding.scope.class == ScopeClass::Test)
                    && execution.as_ref().is_some_and(|row| {
                        row.settles
                            && row.reason.is_none()
                            && row.binding_id.as_deref() == Some(stored.id.as_str())
                    });
                runs.push(RecordedCommandRun {
                    run_id: stored.id.clone(),
                    // Binding sequence starts at zero. The typed check uses
                    // one-based sequence to distinguish a missing receipt.
                    seq: (stored.seq.max(0) as u64).saturating_add(1),
                    version_id: version_id.into(),
                    tree_hash: tree_hash.into(),
                    command: command.command.clone(),
                    exit_code: binding.exit_code,
                    tally: test_tally::of_binding(ledger, &stored.id)?,
                    // The existing tally table has no `errors` field. Do
                    // not manufacture a zero from the exit status.
                    errors: None,
                    complete,
                });
            }
            let census = RunCensus {
                version_id: version_id.into(),
                tree_hash: tree_hash.into(),
                complete: true,
                runs,
            };
            Ok(Some(check_command_passes(
                version_id, tree_hash, command, &census,
            )))
        }
        _ => Ok(None),
    }
}

/// Resolve a version twice, by commit and tree, so a changing ref never
/// silently borrows a run recorded for a different tree.
fn exact_commit(repo: &Path, version_id: &str, tree_hash: &str) -> Option<String> {
    fn git_oid(repo: &Path, spec: &str) -> Option<String> {
        let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
            .args(["rev-parse", "--verify", "--end-of-options", spec])
            .current_dir(repo)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let oid = std::str::from_utf8(&output.stdout).ok()?.trim();
        (oid.len() >= 40 && oid.chars().all(|ch| ch.is_ascii_hexdigit())).then(|| oid.to_owned())
    }
    let commit = git_oid(repo, &format!("{version_id}^{{commit}}"))?;
    (git_oid(repo, &format!("{commit}^{{tree}}"))?.as_str() == tree_hash).then_some(commit)
}
