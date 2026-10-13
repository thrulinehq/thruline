//! Explicit local continuity acknowledgement; no automatic agent resumption.
use std::io::Read;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thruline_core::spec::event::EventId;
use thruline_ledger::{Ledger, RepoId};
use thruline_policy::acceptance::Evaluation;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    version: u32,
    workspace: String,
    branch: Option<String>,
    commit: Option<String>,
    acceptance_hash: String,
    evaluation: Evaluation,
}

fn git_reference(repo: &Path, args: &[&str]) -> Result<Option<String>> {
    let mut command = Command::new("git");
    thruline_core::git_isolation::isolate_config(&mut command);
    let output = command
        .current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args(args)
        .output()?;
    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8(output.stdout)?
                .trim_end_matches('\n')
                .into(),
        )),
        Some(1) => Ok(None), // detached branch or an unborn HEAD
        _ => anyhow::bail!("Git context could not be read for this handoff"),
    }
}

pub fn packet(repo: &Path, body: &Value) -> Result<Packet> {
    anyhow::ensure!(
        body["accepted"] == true,
        "accept requirements before exporting a resumable packet"
    );
    Ok(Packet {
        version: 1,
        workspace: RepoId::from_key(&repo.to_string_lossy()).as_str().into(),
        branch: git_reference(repo, &["symbolic-ref", "--quiet", "HEAD"])?,
        commit: git_reference(repo, &["rev-parse", "--verify", "--quiet", "HEAD"])?,
        acceptance_hash: body["acceptance_hash"]
            .as_str()
            .context("missing acceptance")?
            .into(),
        evaluation: serde_json::from_value(body["evaluation"].clone())?,
    })
}

pub fn digest(packet: &Packet) -> Result<String> {
    Ok(format!(
        "sha256:{}",
        thruline_core::chain::sha256_hex(&thruline_core::spec::canonical::canonical_bytes(packet)?)
    ))
}

pub fn acknowledge(
    repo: &Path,
    db: &Path,
    file: &Path,
    path: &Path,
    reviewed: &str,
    actor: &str,
) -> Result<Value> {
    anyhow::ensure!(
        !actor.trim().is_empty() && actor.len() <= 128 && !actor.chars().any(char::is_control),
        "invalid local acknowledgement label"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 1024 * 1024, "handoff exceeds 1 MiB");
    let export: Value = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
        export["schema_version"] == 1 && export["shape"] == "thruline.acceptance.handoff",
        "unsupported handoff format"
    );
    let incoming: Packet = serde_json::from_value(export["packet"].clone())?;
    anyhow::ensure!(incoming.version == 1, "unsupported handoff packet version");
    let actual_digest = digest(&incoming)?;
    anyhow::ensure!(
        actual_digest == reviewed && export["packet_digest"] == actual_digest,
        "handoff differs from the reviewed packet digest"
    );
    let body = crate::acceptance_cmd::status_document(repo, db, file)?;
    let current = packet(repo, &body)?;
    anyhow::ensure!(incoming == current, "handoff workspace, branch, commit, accepted requirements or current evidence differ; inspect acceptance status and export a current packet");
    let ledger = Ledger::open(db)?;
    let repo_id = RepoId::from_key(&repo.to_string_lossy());
    anyhow::ensure!(
        ledger
            .acceptance_history(&repo_id)?
            .last()
            .is_some_and(|r| r.hash == incoming.acceptance_hash),
        "acceptance changed before acknowledgement"
    );
    let session = format!("handoff-{}", EventId::generate());
    ledger.upsert_session(
        &session,
        &repo_id,
        "acceptance-handoff-v1",
        None,
        false,
        None,
    )?;
    crate::acceptance_exec::append(
        &ledger,
        &repo_id,
        &session,
        json!({"version":1,
        "note":"acceptance_handoff_acknowledged", "packet":incoming,"packet_digest":actual_digest,"acknowledged_by":actor}),
    )?;
    let head = ledger.verify_session(&session)?;
    Ok(
        json!({"handoff_confirmed":true,"acknowledgement":"explicit local identity/evidence comparison; no agent was automatically resumed",
        "packet_digest":actual_digest,"acknowledgement_session":session,"chain_head":head.head_hash,
        "state":body["state"],"evaluation":body["evaluation"],"next_action":body["next_action"]}),
    )
}
