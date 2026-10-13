//! P6-2 (design record §3.2) — **The Read**: what opening one piece of
//! recorded work did, step by step, in the engine's own numbers.
//!
//! ## Address
//!
//! | method | path | what it does |
//! |---|---|---|
//! | GET | `/api/read?session=<id>` | the steps of reading that work's record: each step's name, its real count and its measured time, or why it has none |
//!
//! A **read** address. It writes nothing to any store and nothing to the
//! project; the only thing it keeps is an in-memory note of the last complete
//! read of each session, for as long as this HQ runs.
//!
//! ## Where the steps come from
//!
//! The steps are the verdict pipeline's own phase log — the one
//! `thruline claims --json --phases` prints to stderr as each phase finishes
//! (`claims_cmd::PhaseReporter`), with the pipeline's own counts and measured
//! wall times. This module runs that command exactly as `GET /api/phases`
//! does and **does not touch the pipeline**: nothing in `claims_cmd` changes,
//! so `claims --json` and `explain --json` are the bytes they were.
//!
//! What this module adds is what the pipeline does not say about itself:
//!
//! * **A step that failed.** The pipeline reports a phase only when it
//!   finishes, in a fixed order, and stops at the first error. So when the
//!   command exits with an error, the first phase of [`PIPELINE`] it did not
//!   report is the one that was in progress: it is `failed`, with the error
//!   the command printed (masked, then bounded) as its reason.
//! * **What depended on it.** Every phase that needs a failed, unmeasured or
//!   withheld phase ([`PIPELINE`]'s second column) is `withheld`: no count, no
//!   detail, no time, and the phase it was waiting on. A phase that did not run
//!   and needed nothing that failed is `not_run`: no result at all.
//! * **A step that could not be measured.** A phase reported with
//!   `elapsed_ms: null` is `unmeasured`, and its dependents are withheld too.
//! * **A step that ran and could not compute its fact (T8).** A phase printed
//!   with a typed `failed` slug — today `workspace_since` with
//!   `workspace_unreadable`, when git could not say which paths changed after
//!   a cited check — is `failed` with that slug as its reason and no result of
//!   its own, and what needs it is withheld. It is never `done`.
//! * **A read that was already done.** Before anything runs, the record this
//!   read would examine is fingerprinted ([`fingerprint`]). When the last
//!   complete read of this session in this HQ had the same fingerprint, the
//!   pipeline is **not** run again and nothing is replayed: the answer is one
//!   step, the comparison itself, with its own count and measured time, and
//!   the time of the earlier read.
//!
//! No step is invented: every row this route serves is either a line the
//! pipeline printed or the one comparison this route itself performed and
//! timed. Streaming the steps while they happen is not built — the answer
//! arrives when the read finishes, and the screen says so.

use std::collections::HashMap;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::{bounded_stderr, respond, respond_with, session_param, State};

/// The served shape's name. A consumer refuses anything else.
pub(super) const READ_CONTRACT: &str = "thruline.read.v1";

/// The one step this route performs itself: comparing the record with the
/// last complete read. It is served as a row only when it decided the answer
/// (a cached read), because only then is it the whole of what happened.
pub(super) const COMPARE_STEP: &str = "compare_record";

/// The verdict pipeline's phases, in the order it reports them, each with the
/// phases whose result it needs. The order is `claims_cmd::Phase`'s documented
/// order; the dependencies are read off `session_claims_reporting`:
///
/// * `find_claims` reads the recorded steps (`read_record`);
/// * `match_proof` and `workspace_since` walk the same steps and the check rows;
/// * `run_verifiers` judges every claim against the checks and the workspace;
/// * `write_up` writes what `run_verifiers` concluded.
pub(super) const PIPELINE: [(&str, &[&str]); 6] = [
    ("read_record", &[]),
    ("find_claims", &["read_record"]),
    ("match_proof", &["read_record"]),
    ("workspace_since", &["read_record"]),
    (
        "run_verifiers",
        &["find_claims", "match_proof", "workspace_since"],
    ),
    ("write_up", &["run_verifiers"]),
];

/// The last complete read of each session in this HQ. Memory only: a restart
/// forgets it, and the first read after a restart is a full one.
#[derive(Default)]
pub(super) struct ReadCache {
    entries: Mutex<HashMap<String, Earlier>>,
}

impl ReadCache {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone)]
struct Earlier {
    fingerprint: String,
    read_at: String,
}

/// `GET /api/read?session=<id>`.
pub(super) fn read(state: &State, query: &str, stream: &mut TcpStream) -> Result<()> {
    let session = match session_param(query) {
        Ok(Some(session)) => session,
        Ok(None) => return refuse(stream, "session_required"),
        Err(problem) => return refuse(stream, problem),
    };
    let (db, repo) = sources(state);

    // The comparison, timed. A record that cannot be fingerprinted is never
    // "unchanged": the read goes ahead in full.
    let compare_started = Instant::now();
    let print = fingerprint(&db, &repo, &session);
    let compare_ms = u64::try_from(compare_started.elapsed().as_millis()).ok();
    let earlier = state
        .read_cache
        .entries
        .lock()
        .ok()
        .and_then(|entries| entries.get(&session).cloned());

    if let (Ok(print), Some(earlier)) = (&print, &earlier) {
        if print.digest == earlier.fingerprint {
            let body = json!({
                "contract": READ_CONTRACT,
                "session": session,
                "cached": true,
                "read_at": earlier.read_at,
                "complete": true,
                "reason": Value::Null,
                "phases": [{
                    "phase": COMPARE_STEP,
                    "status": "done",
                    "count": print.steps,
                    "detail": "",
                    "elapsed_ms": compare_ms,
                }],
                "compared": {
                    "outcome": "unchanged",
                    "elapsed_ms": compare_ms,
                    "fingerprint": print.digest,
                },
            });
            return respond_with(
                stream,
                200,
                "application/json; charset=utf-8",
                &[("X-ThruLine-Read", "cached".to_string())],
                &serde_json::to_vec(&body)?,
            );
        }
    }

    let read_at = thruline_core::ingest::iso8601_utc_now();
    let mut command = std::process::Command::new(&state.cli_exe);
    command.arg("--db").arg(&state.db).args([
        "claims",
        "--json",
        "--phases",
        "--session",
        &session,
    ]);
    if let Some(project) = &state.verification {
        command
            .arg("--exec-db")
            .arg(&project.db)
            .current_dir(&project.repo);
    }
    let output = command
        .stdin(std::process::Stdio::null())
        .output()
        .context("running `thruline claims --json --phases`")?;
    if super::child_reported_store_held(&output) {
        return super::store_held(state, stream);
    }
    let code = output.status.code();
    let completed = matches!(code, Some(0) | Some(3));

    let mut emitted: Vec<Value> = Vec::new();
    let mut said: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(value) if value.get("phase").and_then(Value::as_str).is_some() => {
                emitted.push(value)
            }
            _ => said.push(line.to_string()),
        }
    }
    let reason = if completed {
        None
    } else if said.is_empty() {
        Some(format!(
            "the read stopped with exit {}",
            code.map(|c| c.to_string()).unwrap_or_else(|| "-".into())
        ))
    } else {
        Some(bounded_stderr(&said.join("\n")))
    };
    let phases = assemble(&emitted, completed, reason.as_deref());
    let whole = completed && phases.iter().all(|p| p["status"] == "done");

    // Only a read that finished every step with a measured time is kept to
    // compare against, and it replaces whatever was kept. A failed or partial
    // read is never "read earlier" — and it FORGETS the earlier one, so that
    // "nothing changed since" always means since the most recent read: a
    // record that changed and changed back is read again, not called unchanged.
    if let Ok(mut entries) = state.read_cache.entries.lock() {
        match (whole, &print) {
            (true, Ok(print)) => {
                entries.insert(
                    session.clone(),
                    Earlier {
                        fingerprint: print.digest.clone(),
                        read_at: read_at.clone(),
                    },
                );
            }
            _ => {
                entries.remove(&session);
            }
        }
    }

    let body = json!({
        "contract": READ_CONTRACT,
        "session": session,
        "cached": false,
        "read_at": read_at,
        "complete": completed,
        "reason": reason,
        "phases": phases,
        "compared": {
            "outcome": match (&print, &earlier) {
                (Err(_), _) => "unreadable",
                (Ok(_), None) => "first",
                (Ok(_), Some(_)) => "changed",
            },
            "elapsed_ms": compare_ms,
            "fingerprint": print.as_ref().ok().map(|p| p.digest.clone()),
        },
    });
    let raw = serde_json::to_vec(&body)?;
    let served = match super::present_session_projection(state, &raw) {
        Ok(served) => served,
        Err(_) => {
            return respond(
                stream,
                503,
                "application/json; charset=utf-8",
                br#"{"error":"exclusion_policy_unavailable"}"#,
            )
        }
    };
    respond_with(
        stream,
        200,
        "application/json; charset=utf-8",
        &[
            ("X-ThruLine-Read", "fresh".to_string()),
            (
                "X-ThruLine-Projection",
                "claims --json --phases".to_string(),
            ),
            (
                "X-ThruLine-Exit",
                code.map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
            ),
        ],
        &served,
    )
}

fn refuse(stream: &mut TcpStream, problem: &str) -> Result<()> {
    respond(
        stream,
        400,
        "application/json; charset=utf-8",
        serde_json::to_vec(&json!({ "error": problem }))?.as_slice(),
    )
}

/// The execution store and the repository the pipeline will read: the same
/// ones the `claims` child process resolves (its `--exec-db` and working
/// directory in local-verification mode; otherwise the default store and the
/// directory this server runs in, which the child inherits).
fn sources(state: &State) -> (PathBuf, PathBuf) {
    match &state.verification {
        Some(project) => (project.db.clone(), project.repo.clone()),
        None => (
            thruline_ledger::Ledger::default_path().unwrap_or_else(|_| PathBuf::from("exec.db")),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        ),
    }
}

/// What a read would examine, reduced to one digest.
pub(super) struct Fingerprint {
    pub(super) digest: String,
    /// The recorded steps compared — the same number `read_record` counts.
    pub(super) steps: u64,
}

/// Everything the verdict pipeline reads for one session, as a SHA-256:
///
/// * the session's recorded steps — their number and the last one's position
///   and chain hash. The steps are append-only (the store's triggers) and
///   hash-chained, so the last link commits to every step before it;
/// * the session's capture state and its stated reason;
/// * every check row recorded for the session, whole;
/// * the repository's claim pattern table (`.thruline/claims.yml`), whole, or
///   its absence;
/// * the repository path itself.
///
/// NOT covered, and said so where the answer is shown: git objects the
/// pipeline reads by their recorded hash. They are content-addressed, so the
/// same hash is the same bytes; an object that disappears from the repository
/// between two reads is not noticed here.
pub(super) fn fingerprint(db: &Path, repo: &Path, session: &str) -> Result<Fingerprint> {
    use sha2::{Digest, Sha256};
    if !db.is_file() {
        anyhow::bail!("no execution store at {}", db.display());
    }
    let ledger = thruline_ledger::Ledger::open_read_only(db)
        .with_context(|| format!("opening {} to compare", db.display()))?;
    let steps = ledger.session_events(session)?.len() as u64;
    let last = ledger.last_event(session)?;
    let checks = ledger.checks_for_session(session)?;
    let degraded = ledger.capture_degraded(session)?;
    let degraded_reason = ledger.degraded_reason(session)?;
    let table = std::fs::read(repo.join(thruline_claims::Table::PATH)).ok();

    let mut h = Sha256::new();
    let mut part = |bytes: &[u8]| {
        h.update((bytes.len() as u64).to_be_bytes());
        h.update(bytes);
    };
    part(b"thruline-read-fingerprint-v1");
    part(session.as_bytes());
    part(&steps.to_be_bytes());
    match &last {
        Some((seq, hash)) => {
            part(&seq.to_be_bytes());
            part(hash.as_bytes());
        }
        None => part(b"no-steps"),
    }
    part(&[u8::from(degraded)]);
    part(degraded_reason.as_deref().unwrap_or("").as_bytes());
    part(&(checks.len() as u64).to_be_bytes());
    for row in &checks {
        part(format!("{row:?}").as_bytes());
    }
    match &table {
        Some(bytes) => part(bytes),
        None => part(b"no-claim-table"),
    }
    part(repo.to_string_lossy().as_bytes());
    let digest = h
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(Fingerprint { digest, steps })
}

/// The served rows: every phase the pipeline printed, in its order, then every
/// phase of [`PIPELINE`] it did not print, each with the status the rules in
/// this module's header give it. Pure, so the rules are testable without a
/// process.
///
/// * `completed` — the command exited 0 or 3 (a policy answer is a finish).
/// * `reason` — what the command said when it did not complete.
pub(super) fn assemble(emitted: &[Value], completed: bool, reason: Option<&str>) -> Vec<Value> {
    let mut rows: Vec<Value> = emitted
        .iter()
        .map(|p| {
            let timed = p.get("elapsed_ms").is_some_and(Value::is_u64);
            // T8: the pipeline's own typed failure. A slug, never text, so it
            // is served as it came; the row shows no result of its own.
            if let Some(slug) = p.get("failed").and_then(Value::as_str) {
                return json!({
                    "phase": p.get("phase").cloned().unwrap_or(Value::Null),
                    "status": "failed",
                    "count": Value::Null,
                    "detail": Value::Null,
                    "elapsed_ms": Value::Null,
                    "reason": slug,
                });
            }
            json!({
                "phase": p.get("phase").cloned().unwrap_or(Value::Null),
                "status": if timed { "done" } else { "unmeasured" },
                "count": p.get("count").cloned().unwrap_or(Value::Null),
                "detail": p
                    .get("detail")
                    .and_then(Value::as_str)
                    .map(|d| Value::String(thruline_core::redact::mask_recorded(d).0))
                    .unwrap_or(Value::Null),
                "elapsed_ms": p.get("elapsed_ms").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    let printed = |id: &str| rows.iter().any(|r| r["phase"] == id);
    let mut missing: Vec<&str> = PIPELINE
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !printed(id))
        .collect();
    let mut failed_one = false;
    for id in missing.drain(..) {
        let row = if !completed && !failed_one {
            failed_one = true;
            json!({
                "phase": id,
                "status": "failed",
                "count": Value::Null,
                "detail": Value::Null,
                "elapsed_ms": Value::Null,
                "reason": reason.unwrap_or(""),
            })
        } else {
            json!({
                "phase": id,
                "status": "not_run",
                "count": Value::Null,
                "detail": Value::Null,
                "elapsed_ms": Value::Null,
            })
        };
        rows.push(row);
    }
    // Withholding, in pipeline order so it carries down the chain: a phase
    // that needs one that failed, could not be measured, or was itself
    // withheld, shows nothing of its own.
    for (id, needs) in PIPELINE.iter() {
        let status_of = |rows: &[Value], of: &str| {
            rows.iter()
                .find(|r| r["phase"] == of)
                .and_then(|r| r["status"].as_str().map(str::to_string))
        };
        let blocker = needs.iter().find(|need| {
            matches!(
                status_of(&rows, need).as_deref(),
                Some("failed" | "unmeasured" | "withheld")
            )
        });
        let Some(blocker) = blocker else { continue };
        if let Some(row) = rows.iter_mut().find(|r| r["phase"] == *id) {
            if matches!(row["status"].as_str(), Some("done" | "not_run")) {
                *row = json!({
                    "phase": id,
                    "status": "withheld",
                    "count": Value::Null,
                    "detail": Value::Null,
                    "elapsed_ms": Value::Null,
                    "withheld_by": blocker,
                });
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(id: &str, count: u64, ms: u64) -> Value {
        json!({"phase": id, "count": count, "detail": "", "elapsed_ms": ms})
    }

    fn statuses(rows: &[Value]) -> Vec<(String, String)> {
        rows.iter()
            .map(|r| {
                (
                    r["phase"].as_str().unwrap_or("").to_string(),
                    r["status"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn a_complete_read_serves_every_printed_phase_and_nothing_else() {
        let printed: Vec<Value> = PIPELINE
            .iter()
            .enumerate()
            .map(|(i, (id, _))| done(id, i as u64, i as u64))
            .collect();
        let rows = assemble(&printed, true, None);
        assert_eq!(rows.len(), 6);
        for (row, (id, _)) in rows.iter().zip(PIPELINE.iter()) {
            assert_eq!(row["phase"], *id);
            assert_eq!(row["status"], "done");
        }
        assert_eq!(
            rows[4]["count"], 4,
            "the count is the pipeline's, untouched"
        );
        assert_eq!(
            rows[4]["elapsed_ms"], 4,
            "the time is the pipeline's, untouched"
        );
    }

    #[test]
    fn a_failed_phase_withholds_what_needs_it_and_what_did_not_run_shows_nothing() {
        let rows = assemble(
            &[done("read_record", 3, 6)],
            false,
            Some("loading the claim pattern table: claims.yml:1: expected `version:`"),
        );
        assert_eq!(
            statuses(&rows),
            vec![
                ("read_record".into(), "done".into()),
                ("find_claims".into(), "failed".into()),
                ("match_proof".into(), "not_run".into()),
                ("workspace_since".into(), "not_run".into()),
                ("run_verifiers".into(), "withheld".into()),
                ("write_up".into(), "withheld".into()),
            ]
        );
        assert!(rows[1]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("claims.yml")));
        assert_eq!(rows[4]["withheld_by"], "find_claims");
        assert_eq!(rows[5]["withheld_by"], "run_verifiers");
        for row in &rows[2..] {
            assert_eq!(row["count"], Value::Null, "{row}");
            assert_eq!(row["elapsed_ms"], Value::Null, "{row}");
            assert_eq!(row["detail"], Value::Null, "{row}");
        }
    }

    #[test]
    fn a_phase_the_engine_says_could_not_compute_is_failed_with_its_slug_never_done() {
        let mut printed: Vec<Value> = PIPELINE.iter().map(|(id, _)| done(id, 1, 1)).collect();
        printed[3]["failed"] = json!("workspace_unreadable");
        let rows = assemble(&printed, true, None);
        assert_eq!(rows[3]["status"], "failed", "{}", rows[3]);
        assert_eq!(rows[3]["reason"], "workspace_unreadable");
        for key in ["count", "detail", "elapsed_ms"] {
            assert!(rows[3][key].is_null(), "{}", rows[3]);
        }
        assert_eq!(rows[4]["status"], "withheld");
        assert_eq!(rows[4]["withheld_by"], "workspace_since");
        assert_eq!(rows[5]["status"], "withheld");
    }

    #[test]
    fn a_phase_the_engine_could_not_time_withholds_its_dependents_even_when_they_ran() {
        let mut printed: Vec<Value> = PIPELINE.iter().map(|(id, _)| done(id, 1, 1)).collect();
        printed[1]["elapsed_ms"] = Value::Null;
        let rows = assemble(&printed, true, None);
        assert_eq!(rows[1]["status"], "unmeasured");
        assert_eq!(rows[1]["count"], 1, "its count was measured and is kept");
        assert_eq!(
            rows[2]["status"], "done",
            "match_proof does not need find_claims"
        );
        assert_eq!(rows[4]["status"], "withheld");
        assert_eq!(rows[4]["withheld_by"], "find_claims");
        assert_eq!(rows[4]["count"], Value::Null);
        assert_eq!(rows[5]["status"], "withheld");
    }

    #[test]
    fn the_first_phase_failing_withholds_everything() {
        let rows = assemble(&[], false, Some("no execution store at /x"));
        assert_eq!(rows[0]["status"], "failed");
        for row in &rows[1..] {
            assert_eq!(row["status"], "withheld", "{row}");
        }
    }

    #[test]
    fn a_detail_is_masked_before_it_is_served() {
        let secret = "ghp_0123456789abcdefghijklmnopqrstuvwxyzAB";
        let mut row = done("read_record", 1, 1);
        row["detail"] = json!(format!("session x · {secret}"));
        let rows = assemble(&[row], true, None);
        assert!(!rows[0]["detail"].as_str().unwrap_or("").contains(secret));
    }

    #[test]
    fn nothing_is_invented_for_a_phase_the_pipeline_never_names() {
        let rows = assemble(&[done("read_record", 1, 1)], true, None);
        assert_eq!(rows.len(), 6, "only PIPELINE's own phases are ever added");
        assert!(rows[1..].iter().all(|r| r["status"] == "not_run"));
    }

    #[test]
    fn the_fingerprint_moves_with_the_rule_table_and_refuses_a_missing_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("exec.db");
        assert!(
            fingerprint(&db, dir.path(), "s").is_err(),
            "no store, no fingerprint"
        );
        drop(thruline_ledger::Ledger::open(&db).expect("store"));
        let before = fingerprint(&db, dir.path(), "s")
            .expect("fingerprint")
            .digest;
        assert_eq!(
            before,
            fingerprint(&db, dir.path(), "s").expect("again").digest,
            "the same record fingerprints the same"
        );
        std::fs::create_dir_all(dir.path().join(".thruline")).expect("dir");
        std::fs::write(dir.path().join(".thruline/claims.yml"), "version: 1\n").expect("table");
        let after = fingerprint(&db, dir.path(), "s")
            .expect("fingerprint")
            .digest;
        assert_ne!(before, after, "a new rule table is a changed record");
    }
}
