//! Source-backed claim selection and exact wrapper receipts for J adapters.
//! Selectors are retained verbatim. Neither an accepted selector nor an HQ
//! version bucket can manufacture an event/tree or agent/run-session link.

use std::path::Path;

use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use crate::chain::sha256_hex;
use crate::contract::{parse_claim, ClaimPolarity, Family};
use crate::error::Result;
use crate::evidence_binding::{self, EvidenceState, ScopeClass, WsidScopeDecl};
use crate::examined_inputs::{self, SubjectKind, Uncommitted};
use crate::extract::{ClaimExtractor, MarkerExtractor, EXTRACTOR_NAME};
use crate::ledger::Ledger;
use crate::run_execution;
use crate::test_tally;
use crate::workspace;

use super::checks_claims::{ClaimCensus, ClaimKind, ClaimReceipt, ClaimRun, RunOrigin};
use super::params::CheckParams;
use super::record_source;

type StoredClaimRow = (String, String, Option<i64>, Option<i64>, String);

#[derive(Debug, Clone)]
pub struct ClaimRecordProjection {
    /// Exact accepted selectors: decision/basis, selected runs, origin and edits
    /// remain accessible even when their source linkage cannot be verified.
    pub selectors: Value,
    pub census: ClaimCensus,
    /// Raw event and independently verified execution identities; these are
    /// factual observations, not a completeness promise.
    pub sources: Vec<Value>,
    pub missing: Vec<&'static str>,
}

/// Inspect accepted selectors without projecting missing facts into a census.
pub fn project_claim_at(
    ledger: &Ledger,
    repo: &Path,
    ledger_project_id: &str,
    version_id: &str,
    tree_hash: &str,
    selector: &CheckParams,
) -> Result<ClaimRecordProjection> {
    let (claim_id, kind, run_ids) = match selector {
        CheckParams::DecisionSatisfied(rule) => (
            rule.claim_id.as_str(),
            ClaimKind::DecisionSatisfied,
            Vec::new(),
        ),
        CheckParams::StaleClaim(rule) => (rule.claim_id.as_str(), ClaimKind::Other, Vec::new()),
        CheckParams::SummedTotals(rule) => (
            rule.claim_id.as_str(),
            ClaimKind::TestsPass,
            rule.run_ids.clone(),
        ),
        CheckParams::EmptyRun(rule) => (
            rule.claim_id.as_str(),
            ClaimKind::TestsPass,
            vec![rule.run_id.clone()],
        ),
        CheckParams::DelegatedRun(rule) => {
            (rule.claim_id.as_str(), ClaimKind::TestsPass, Vec::new())
        }
        CheckParams::EditDuringRun(rule) => ("", ClaimKind::TestsPass, vec![rule.run_id.clone()]),
        _ => ("", ClaimKind::Other, Vec::new()),
    };
    let mut projection = ClaimRecordProjection {
        selectors: serde_json::to_value(selector)?,
        census: ClaimCensus {
            complete: false,
            final_seq: 0,
            claim: ClaimReceipt {
                id: claim_id.into(),
                session_id: String::new(),
                seq: 0,
                // The target selected by a caller is not the claim's recorded version.
                version_id: String::new(),
                tree_hash: String::new(),
                kind,
                reported_passed: None,
                bound_run_id: None,
            },
            decision: None,
            runs: Vec::new(),
            edits: Vec::new(),
        },
        sources: Vec::new(),
        missing: Vec::new(),
    };
    if claim_id.is_empty() {
        projection
            .missing
            .push("J6_requires_a_run_and_edit_adapter_not_a_synthetic_claim");
    } else {
        let row: Option<StoredClaimRow> = ledger
            .conn()
            .query_row(
                "SELECT c.event_id,c.text,c.span_start,c.span_end,c.extractor
             FROM claims c JOIN events e ON e.id=c.event_id WHERE c.id=?1 AND e.project_id=?2",
                params![claim_id, ledger_project_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        if let Some((event_id, text, Some(start), Some(end), extractor)) = row {
            if let Some(source) =
                record_source::verified_event(ledger, ledger_project_id, &event_id)?
            {
                if source.kind == "assistant_text"
                    && source.role.as_deref() == Some("assistant")
                    && extractor == EXTRACTOR_NAME
                {
                    if let Some(full_text) = source.text.as_deref() {
                        let matched =
                            MarkerExtractor
                                .extract(full_text)
                                .into_iter()
                                .find(|claim| {
                                    claim.text == text
                                        && claim.span_start as i64 == start
                                        && claim.span_end as i64 == end
                                        && format!(
                                            "c{}",
                                            &sha256_hex(
                                                format!(
                                                    "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                                                    event_id, extractor, start, end, text
                                                )
                                                .as_bytes()
                                            )[..31]
                                        ) == claim_id
                                });
                        if matched.is_some() {
                            projection.census.claim.session_id = source.session_id.clone();
                            projection.census.claim.seq = source.seq as u64;
                            projection.census.claim.kind = ClaimKind::Other;
                            if let Some(assertion) = parse_claim(&text) {
                                if assertion.family == Family::Test
                                    && assertion.polarity == ClaimPolarity::AssertsSuccess
                                {
                                    projection.census.claim.kind = ClaimKind::TestsPass;
                                    projection.census.claim.reported_passed =
                                        assertion.passed.map(u64::from);
                                }
                            }
                            projection.sources.push(serde_json::json!({
                                "claim_id":claim_id,"event_id":event_id,"session_id":source.session_id,
                                "raw_file":source.raw_file,"raw_line":source.raw_line,"raw_sha256":source.raw_sha256,
                                "chain_hash":source.chain_hash,"span_start":start,"span_end":end,"extractor":extractor,
                            }));
                        }
                    }
                }
            }
        }
        if projection.sources.is_empty() {
            projection
                .missing
                .push("selected_claim_has_no_verified_deterministic_source_span");
        }
    }
    // Inspect only the exact requested runs, preserving their actual session
    // rather than changing it to the assistant's session to make a check pass.
    for run_id in run_ids {
        let Some(mut run) = selected_wrapper_run(ledger, repo, version_id, tree_hash, &run_id)?
        else {
            projection
                .missing
                .push("selected_run_lacks_a_complete_exact_tree_wrapper_receipt");
            continue;
        };
        if run.session_id == projection.census.claim.session_id && !run.session_id.is_empty() {
            run.origin = RunOrigin::SameSession;
        }
        projection.sources.push(serde_json::json!({
            "run_id":run_id,"binding_id":run.id,"source_session":run.session_id,
            "version_id":run.version_id,"tree_hash":run.tree_hash,"passed":run.passed,"failed":run.failed,
            "exit_code":run.exit_code,"started_ms":run.started_ms,"ended_ms":run.ended_ms,
        }));
        projection.census.runs.push(run);
    }
    projection.missing.extend([
        "claim_exact_version_tree_binding",
        "claim_to_run_same_session_binding",
        "complete_closed_session_and_relevant_edit_census",
    ]);
    Ok(projection)
}

/// Existing foreground wrappers can prove one exact execution. They do not
/// provide the enclosing assistant's session, source order, or path census.
fn selected_wrapper_run(
    ledger: &Ledger,
    repo: &Path,
    version_id: &str,
    tree_hash: &str,
    run_id: &str,
) -> Result<Option<ClaimRun>> {
    let Some(stored) = evidence_binding::load(ledger, run_id)? else {
        return Ok(None);
    };
    let binding = &stored.binding;
    let Some(repo_id) = workspace::repo_identity_at(repo)?.repo_id else {
        return Ok(None);
    };
    if binding.repository_identity != repo_id {
        return Ok(None);
    }
    let Some(inputs) = examined_inputs::standing(ledger, SubjectKind::CheckRun, run_id)? else {
        return Ok(None);
    };
    let execution = run_execution::latest(ledger, &binding.source_session)?;
    let complete = inputs.base_revision.value() == Some(version_id)
        && matches!(inputs.uncommitted, Uncommitted::Clean)
        && inputs.source_identity.value().is_some()
        && inputs.source_identity.value() == binding.pre.identity.as_deref()
        && inputs.source_identity.value() == binding.post.identity.as_deref()
        && binding.wsid_scope == WsidScopeDecl::everything()
        && evidence_binding::state_at_binding(binding) == EvidenceState::CurrentStateConfirmed
        && binding.term_signal.is_none()
        && binding.wrapper_defects.is_empty()
        && binding.scope.class == ScopeClass::Test
        && execution.as_ref().is_some_and(|execution| {
            execution.settles
                && execution.reason.is_none()
                && execution.binding_id.as_deref() == Some(run_id)
        });
    // Verify the supplied full commit's tree before labelling the wrapper row.
    if ![version_id, tree_hash]
        .iter()
        .all(|oid| matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Ok(None);
    }
    let commit = crate::git_isolation::isolate_config(&mut std::process::Command::new("git"))
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{version_id}^{{commit}}"),
        ])
        .current_dir(repo)
        .output();
    if !commit.is_ok_and(|output| {
        output.status.success()
            && std::str::from_utf8(&output.stdout).is_ok_and(|actual| actual.trim() == version_id)
    }) {
        return Ok(None);
    }
    let tree = crate::git_isolation::isolate_config(&mut std::process::Command::new("git"))
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{version_id}^{{tree}}"),
        ])
        .current_dir(repo)
        .output();
    if !complete
        || !tree.is_ok_and(|output| {
            output.status.success()
                && std::str::from_utf8(&output.stdout)
                    .is_ok_and(|actual| actual.trim() == tree_hash)
        })
    {
        return Ok(None);
    }
    let tally = test_tally::of_binding(ledger, run_id)?;
    Ok(Some(ClaimRun {
        id: stored.id,
        session_id: binding.source_session.clone(),
        // binding.seq is not an assistant session seq and must never be mixed with one.
        seq: 0,
        version_id: version_id.into(),
        tree_hash: tree_hash.into(),
        origin: RunOrigin::OtherSession,
        passed: tally.as_ref().map(|tally| tally.passed),
        failed: tally.as_ref().map(|tally| tally.failed),
        exit_code: binding.exit_code,
        started_ms: crate::hq::parse_ms(&binding.started_at),
        ended_ms: crate::hq::parse_ms(&binding.bound_at),
        // Whole workspace identity confirms state, not the test runner's path coverage.
        examined_paths: Vec::new(),
        complete,
    }))
}
