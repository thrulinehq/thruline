//! Pack 2 rule evaluation. Stored observations never inherit agent authority.
use crate::{
    acceptance_exec as exec,
    verification_pack::{read_output, Observation},
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path, process::Command};
use thruline_policy::acceptance::{Check, Document, EvidenceKind, Outcome, PackPlan};

pub const UNITTEST: &str = include_str!("verification_unittest.py");
pub fn plan(check: &Check) -> Option<&PackPlan> {
    check.verification.as_ref()?.pack.as_ref()
}
pub fn suite_result(check: &Check, value: &Value) -> (Outcome, String) {
    let Some(PackPlan::Suite {
        directory,
        pattern,
        expected_tests,
        exclusions,
        claimed_counts,
    }) = plan(check)
    else {
        return (Outcome::Unparsed, "No accepted suite identity.".into());
    };
    let unknown = || {
        (Outcome::Unparsed,"The complete discovery and execution record could not be established. Restore the runner evidence and rerun.".into())
    };
    if value["protocol"] != "thruline-unittest/2"
        || value["directory"] != *directory
        || value["pattern"] != *pattern
        || value["loader_errors"]
            .as_array()
            .is_none_or(|a| !a.is_empty())
    {
        return unknown();
    }
    let Some(discovered) = value["discovered"].as_array() else {
        return unknown();
    };
    let Some(rows) = value["rows"].as_array() else {
        return unknown();
    };
    let ids: Option<BTreeSet<_>> = discovered.iter().map(Value::as_str).collect();
    let Some(ids) = ids else {
        return unknown();
    };
    let actual: Option<BTreeSet<_>> = rows.iter().map(|r| r["id"].as_str()).collect();
    let Some(actual) = actual else {
        return unknown();
    };
    if ids.len() != discovered.len()
        || actual.len() != rows.len()
        || !actual.is_subset(&ids)
        || value["tests_run"].as_u64() != Some(rows.len() as u64)
    {
        return unknown();
    }
    let expected: BTreeSet<_> = expected_tests.iter().map(String::as_str).collect();
    if ids != expected {
        return (Outcome::Failed,format!("The discovered tests differ from the accepted manifest. Missing: {:?}; additional: {:?}. Review the manifest or restore the required tests, then rerun.",expected.difference(&ids).collect::<Vec<_>>(),ids.difference(&expected).collect::<Vec<_>>()));
    }
    if ids != actual {
        return unknown();
    }
    if rows
        .iter()
        .any(|r| matches!(r["state"].as_str(), Some("running") | None))
    {
        return unknown();
    }
    let skipped: Vec<_> = rows
        .iter()
        .filter(|r| matches!(r["state"].as_str(), Some("skip" | "expected_failure")))
        .map(|r| r["id"].as_str().unwrap_or(""))
        .collect();
    if skipped.iter().any(|id| !exclusions.contains(*id)) {
        return (Outcome::Failed,format!("Required tests did not execute normally: {skipped:?}. Remove the skip or explicitly accept an exclusion; an exclusion is never a pass."));
    }
    if let Some(counts) = claimed_counts {
        let observed = thruline_policy::acceptance::SuiteCounts {
            passed: rows.iter().filter(|r| r["state"] == "pass").count() as u64,
            failed: rows
                .iter()
                .filter(|r| {
                    matches!(
                        r["state"].as_str(),
                        Some("fail" | "error" | "unexpected_success")
                    )
                })
                .count() as u64,
            skipped: rows.iter().filter(|r| r["state"] == "skip").count() as u64,
            expected_failure: rows
                .iter()
                .filter(|r| r["state"] == "expected_failure")
                .count() as u64,
        };
        if &observed != counts {
            return (Outcome::Failed,format!("The reported counts contradict captured test outcomes. Reported: {counts:?}; observed: {observed:?}. Correct the report and inspect the captured test IDs."));
        }
    }
    if rows.iter().any(|r| {
        matches!(
            r["state"].as_str(),
            Some("fail" | "error" | "unexpected_success")
        )
    }) {
        return (Outcome::Failed,"A required test recorded a failure. Inspect its captured test ID and detail, repair the demonstrated problem, and rerun.".into());
    }
    if rows.iter().any(|r| {
        !matches!(
            r["state"].as_str(),
            Some("pass" | "skip" | "expected_failure")
        )
    }) || rows.iter().all(|r| r["state"] != "pass")
    {
        return unknown();
    }
    (Outcome::Passed,format!("{} accepted tests were discovered; {} passed and {} accepted exclusions are shown separately. This covers this named suite only.",ids.len(),rows.len()-skipped.len(),skipped.len()))
}

pub fn suite_catalog_state(check: &Check, value: &Value, id: &str) -> &'static str {
    let (outcome, reason) = suite_result(check, value);
    if outcome == Outcome::Unparsed {
        return "unknown";
    }
    if reason.starts_with("The discovered tests differ") {
        return "fail";
    }
    if id == "EVD-05" {
        if let Some(PackPlan::Suite {
            claimed_counts: Some(counts),
            ..
        }) = plan(check)
        {
            let rows = value["rows"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            let actual = [
                rows.iter().filter(|r| r["state"] == "pass").count() as u64,
                rows.iter()
                    .filter(|r| {
                        matches!(
                            r["state"].as_str(),
                            Some("fail" | "error" | "unexpected_success")
                        )
                    })
                    .count() as u64,
                rows.iter().filter(|r| r["state"] == "skip").count() as u64,
                rows.iter()
                    .filter(|r| r["state"] == "expected_failure")
                    .count() as u64,
            ];
            if actual
                != [
                    counts.passed,
                    counts.failed,
                    counts.skipped,
                    counts.expected_failure,
                ]
            {
                return "fail";
            }
        }
    }
    if id == "TST-01" && reason.starts_with("Required tests did not execute") {
        return "fail";
    }
    // Execution/count coverage can be complete even when the tests find a defect.
    "pass"
}

pub fn completion_observation(
    repo: &Path,
    db: &Path,
    doc: &Document,
    audit: &Check,
) -> Result<Value> {
    let status =
        crate::acceptance_cmd::status_document(repo, db, &repo.join(".thruline/acceptance.json"))?;
    let mut criteria = vec![];
    for c in &doc.contract().criteria {
        if c.check_ids == [audit.id.clone()] {
            continue;
        }
        let mut checks = vec![];
        for id in &c.check_ids {
            let definition = doc
                .contract()
                .checks
                .iter()
                .find(|k| k.id == *id)
                .context("required check missing")?;
            let run = status["runs"]
                .as_array()
                .and_then(|rows| rows.iter().find(|r| r["check_id"] == *id));
            let suitable = match c.evidence_kind {
                Some(EvidenceKind::Behavior) => definition
                    .verification
                    .as_ref()
                    .is_some_and(|v| !v.cases.is_empty()),
                Some(EvidenceKind::Tests) => {
                    matches!(plan(definition), Some(PackPlan::Suite { .. }))
                }
                Some(EvidenceKind::Build) => matches!(
                    definition.category,
                    thruline_policy::acceptance::Category::Build
                        | thruline_policy::acceptance::Category::Typecheck
                ),
                Some(EvidenceKind::Architecture) => {
                    matches!(plan(definition), Some(PackPlan::Architecture { .. }))
                }
                Some(EvidenceKind::Resource) => {
                    matches!(plan(definition), Some(PackPlan::Resource { .. }))
                }
                Some(EvidenceKind::Artifact) => {
                    matches!(plan(definition), Some(PackPlan::Artifact))
                }
                _ => false,
            };
            // A typed public receipt reference keeps the canonical ULID separate
            // from its namespace. Do not relax capture redaction for opaque IDs.
            let run_ref = run
                .and_then(|r| r["run_id"].as_str())
                .and_then(|id| id.strip_prefix("acceptance-evt_"))
                .map(|id| json!({"namespace":"acceptance-evt_","id":id}));
            checks.push(json!({"check_id":id,"run_ref":run_ref,"current":run.is_some_and(|r|r["current"]==true),"admissible":run.is_some_and(|r|r["admissible"]==true),"outcome":run.map(|r|&r["outcome"]),"suitable":suitable}));
        }
        criteria.push(json!({"id":c.id,"description":c.description,"evidence_kind":c.evidence_kind,"human_acceptance_required":c.human_acceptance_required,"checks":checks}));
    }
    Ok(
        json!({"protocol":"thruline-completion/2","accepted_digest":doc.digest(),"criteria":criteria,"boundary":"Only these accepted criteria. No waiver is inferred and no broader project-correctness claim is established."}),
    )
}
pub fn completion_result(value: &Value) -> (Outcome, String) {
    let Some(criteria) = value["criteria"].as_array() else {
        return (
            Outcome::Unparsed,
            "The accepted criterion list is missing.".into(),
        );
    };
    let mut missing = vec![];
    let mut failed = vec![];
    if criteria.is_empty() {
        return (
            Outcome::Unparsed,
            "No product criteria were available for the completion audit.".into(),
        );
    }
    for c in criteria {
        if c["human_acceptance_required"] == true
            || c["checks"].as_array().is_none_or(|a| a.is_empty())
        {
            missing.push(c["id"].as_str().unwrap_or("unnamed criterion"));
            continue;
        }
        for check in c["checks"].as_array().into_iter().flatten() {
            if check["current"] != true || check["admissible"] != true || check["suitable"] != true
            {
                missing.push(c["id"].as_str().unwrap_or("unnamed criterion"));
            } else if check["outcome"] == "failed" {
                failed.push(c["id"].as_str().unwrap_or("unnamed criterion"));
            } else if check["outcome"] != "passed" {
                missing.push(c["id"].as_str().unwrap_or("unnamed criterion"));
            }
        }
    }
    if !missing.is_empty() {
        missing.sort_unstable();
        missing.dedup();
        let missing = missing.join(", ");
        return(Outcome::Unparsed,format!("Completion remains unknown: {missing} lacks current suitable evidence or a required human decision. Run the missing check or accept a suitable observable criterion; an edit or successful build alone cannot establish behavior."));
    }
    if !failed.is_empty() {
        failed.sort_unstable();
        failed.dedup();
        let failed = failed.join(", ");
        return(Outcome::Failed,format!("Required criteria have observed failing checks: {failed}. Inspect their linked receipts and rerun after repair."));
    }
    (Outcome::Passed,format!("All {} accepted product criteria have current, suitable passing evidence. This says nothing about requirements outside this accepted set.",criteria.len()))
}

pub fn stored_json(db: &Path, o: &Observation) -> Result<Value> {
    let out = o
        .outputs
        .first()
        .context("structured evidence not captured")?;
    let digest = out
        .digest
        .strip_prefix("sha256:")
        .context("invalid evidence digest")?;
    anyhow::ensure!(
        digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid evidence digest"
    );
    Ok(serde_json::from_slice(&read_output(
        &exec::storage(db)?
            .join("outputs")
            .join(format!("{digest}.out")),
        &out.digest,
    )?)?)
}

pub fn test_changes(repo: &Path, baseline: Option<&str>, tree: &str) -> Vec<String> {
    let Some(baseline) = baseline else {
        return vec![
            "The accepted test baseline is unavailable; test-change review is incomplete.".into(),
        ];
    };
    let mut command = Command::new("git");
    thruline_core::git_isolation::isolate_config(&mut command);
    let out = command
        .current_dir(repo)
        .args([
            "diff-tree",
            "-r",
            "--name-only",
            "-z",
            "--no-commit-id",
            baseline,
            tree,
        ])
        .output();
    match out {
        Ok(out) if out.status.success() && out.stdout.len()<=1024*1024=>String::from_utf8_lossy(&out.stdout).split('\0').filter(|p|!p.is_empty() && crate::verification_scope::role(p)=="test").map(|p|format!("Test file changed: {p}. Review its assertions against the accepted requirement. This edit does not by itself establish weakened tests or improper intent.")).collect(),
        _=>vec!["Test-change comparison is unavailable; no weakening conclusion was drawn.".into()],
    }
}
