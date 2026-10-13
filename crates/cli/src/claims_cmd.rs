//! `thruline claims` — BUILD-SPEC §14, and the seam where §5 meets §6.
//!
//! Reads one session out of the ledger, extracts its claims and promises
//! (§5.2), builds the views §6 verifiers take, and prints claim → verdict →
//! evidence → reason.
//!
//! # The transcript is read here, and it decides nothing
//!
//! The only prose this command reads is the `stop` event's
//! `last_assistant_message`, and it reads it to answer one question: *what did
//! the agent claim*. Every verdict below comes from `exec` events, snapshots,
//! `fs_change` events and parsed checks. §0.2, kept by construction: the
//! extractor hands the verifier a `VerifiableClaim`, which cannot carry prose.
//!
//! # A claim kind with no verifier is `unverifiable`, and says which
//!
//! §6.4–§6.6 are not built. Their claims are not silently dropped and are not
//! quietly passed: each gets `unverifiable` naming the verifier that does not
//! exist yet. §6's non-collapse rule applies to missing code exactly as it
//! applies to missing evidence.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::Value;
use thruline_claims::{extract, ClaimKind, Extraction, Source, Table};
use thruline_ledger::Ledger;
use thruline_verify::behavior_unchanged;
use thruline_verify::binding::{
    CheckView, EditView, ParsedView, Reexecution, SessionView, SnapshotView,
};
use thruline_verify::diff_evidence as de;
use thruline_verify::tamper::Language;
use thruline_verify::tests_pass::{self, Inputs};
use thruline_verify::tool_checks;
use thruline_verify::verdict::{Verdict, VerdictRecord};

pub struct Args {
    pub session: Option<String>,
    pub last: bool,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    /// Write one JSON line per pipeline phase to **stderr** as it completes.
    /// Stdout is untouched, so `claims --json` stays byte-identical.
    pub phases: bool,
}

/// One phase of the verdict pipeline, reported when its real work completes.
///
/// The count and detail are the pipeline's own numbers at that point — events
/// read, claims extracted, checks cited — never a display estimate, and
/// `elapsed_ms` is the measured wall time of that phase's work, not a pacing
/// value. The six phases are emitted in a fixed order: `read_record`,
/// `find_claims`, `match_proof`, `workspace_since`, `run_verifiers`, `write_up`.
///
/// **A detail carries ids and counts, never recorded text.** The phase log is
/// served to the Verdict Panel verbatim (`uiserve.rs` `phases()`), outside
/// every mask: a quoted claim could put a record's credential on the page when
/// the record held one unmasked, and its cut by character count knew nothing
/// of `[masked: …]` markers; a `git diff-tree` path was never masked at all.
/// Claims are named by id; the panel reads their text from `/api/claims`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    pub phase: &'static str,
    pub count: u64,
    pub detail: String,
    /// `None` when the measured time does not fit — **unknown, written as
    /// JSON `null`**, so the log never shows a duration it did not measure.
    pub elapsed_ms: Option<u64>,
    /// T8 — set when the phase ran and could not compute what it exists to
    /// compute, as a closed slug (never a path, never recorded text). Today
    /// only `workspace_since`, with `workspace_unreadable`: the tree moved after
    /// a cited check and `git` could not say which paths changed. Written to
    /// the JSON line only when set, so every other line is byte-identical.
    pub failed: Option<&'static str>,
}

/// Where phase records go. The seam is passed by `&mut` through
/// `session_claims` → `session_view` → `judge`; every other caller of those
/// functions (`explain`, `tui`, `receipt`, `gate`, `policy`, `reexec`, `demo`)
/// gets [`PhaseReporter::none`] through the unchanged wrappers, and only
/// `claims --phases` installs a sink. Reporting never touches a verdict: the
/// sink receives a copy of a count the pipeline already had.
pub struct PhaseReporter<'a> {
    sink: Option<&'a mut dyn FnMut(&Phase)>,
}

impl<'a> PhaseReporter<'a> {
    /// The no-op reporter every non-`--phases` path uses.
    pub fn none() -> Self {
        PhaseReporter { sink: None }
    }

    pub fn to(sink: &'a mut dyn FnMut(&Phase)) -> PhaseReporter<'a> {
        PhaseReporter { sink: Some(sink) }
    }

    fn report(&mut self, phase: &'static str, count: u64, detail: String, elapsed: Duration) {
        self.report_outcome(phase, count, detail, elapsed, None);
    }

    fn report_outcome(
        &mut self,
        phase: &'static str,
        count: u64,
        detail: String,
        elapsed: Duration,
        failed: Option<&'static str>,
    ) {
        if let Some(sink) = self.sink.as_mut() {
            sink(&Phase {
                phase,
                count,
                detail,
                elapsed_ms: u64::try_from(elapsed.as_millis()).ok(),
                failed,
            });
        }
    }
}

/// One phase as its `--phases` JSON object. `elapsed_ms: None` is `null`;
/// `failed` appears only when the phase could not compute its fact.
fn phase_json(phase: &Phase) -> Value {
    let mut line = serde_json::json!({
        "phase": phase.phase,
        "count": phase.count,
        "detail": phase.detail,
        "elapsed_ms": phase.elapsed_ms,
    });
    if let Some(slug) = phase.failed {
        line["failed"] = Value::String(slug.to_string());
    }
    line
}

/// The `--phases` transport: one compact JSON object per line on stderr.
fn print_phase(phase: &Phase) {
    eprintln!("{}", phase_json(phase));
}

/// One session's claims and what the record said about them.
///
/// Extracted so `thruline policy` evaluates **the same** claims and the
/// same verdicts `thruline claims` prints. A gate that ran a second, parallel
/// extraction would be a gate that can disagree with the tool the developer
/// used to check their work before pushing, and the first time it did nobody
/// would know which one was wrong.
pub struct SessionClaims {
    pub session: String,
    pub extraction: Extraction,
    pub verdicts: Vec<(usize, VerdictRecord)>,
    pub view: SessionView,
    pub repo: PathBuf,
    pub ledger: Ledger,
}

/// Runs §5's extraction and §6's verifiers over one session.
pub fn session_claims(
    session: Option<String>,
    exec_db: Option<PathBuf>,
    repo: Option<PathBuf>,
) -> Result<SessionClaims> {
    session_claims_reporting(session, exec_db, repo, &mut PhaseReporter::none())
}

/// [`session_claims`], reporting each phase as it completes.
pub fn session_claims_reporting(
    session: Option<String>,
    exec_db: Option<PathBuf>,
    repo: Option<PathBuf>,
    phases: &mut PhaseReporter<'_>,
) -> Result<SessionClaims> {
    let read_started = Instant::now();
    let db = match exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    if !db.is_file() {
        anyhow::bail!("no execution store at {}", db.display());
    }
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let session = match session {
        Some(s) => s,
        None => last_session(&ledger)?.context("no session in the store")?,
    };
    let repo = repo
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let events = ledger
        .session_events(&session)
        .with_context(|| format!("reading session {session}"))?;
    phases.report(
        "read_record",
        events.len() as u64,
        format!("session {session} · {} events", events.len()),
        read_started.elapsed(),
    );
    let find_started = Instant::now();
    let table = Table::load(&repo).context("loading the claim pattern table")?;
    let sources: Vec<Source> = events
        .iter()
        .filter_map(|e| {
            let text = match e.kind().as_str().as_str() {
                "stop" => e.payload().get("last_assistant_message")?.as_str()?,
                _ => return None,
            };
            Some(Source {
                event_id: e.id().to_string(),
                session_id: session.clone(),
                text: text.to_string(),
            })
        })
        .collect();
    let mut extraction = extract(&sources, &table);
    // Ids, not text: see [`Phase`]. The ids are the receipts `claims --json`
    // carries under `claims[].id`, so every one resolves to its claim there.
    phases.report(
        "find_claims",
        extraction.claims.len() as u64,
        if extraction.claims.is_empty() {
            "no completion claim in this session".to_string()
        } else {
            extraction
                .claims
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>()
                .join(" · ")
        },
        find_started.elapsed(),
    );
    let view = session_view_reporting(&session, &events, &ledger, &repo, phases)?;
    let judged = judge_reporting(&session, &events, &view, &repo, phases)?;
    // **One list, not two.** `subject_of` and the printer both zip
    // `extraction.claims` against `verdicts` by index; since F5's floor adds a
    // claim that `extract` alone does not produce, the two lists are made the
    // same list here rather than left to agree by coincidence.
    extraction.claims = judged.iter().map(|(c, _)| c.clone()).collect();
    let verdicts: Vec<(usize, VerdictRecord)> = judged
        .iter()
        .enumerate()
        .map(|(i, (_, v))| (i, v.clone()))
        .collect();
    Ok(SessionClaims {
        session,
        extraction,
        verdicts,
        view,
        repo,
        ledger,
    })
}

pub fn run(args: Args) -> Result<ExitCode> {
    // `--last` is the default and is accepted explicitly so a script can say
    // what it means; naming a session overrides it.
    let _ = args.last;
    // **The same function `thruline policy` calls.** Two extraction paths would be
    // two answers, and the first time they disagreed nobody would know which
    // one the gate had used.
    let mut sink = print_phase;
    let mut phases = if args.phases {
        PhaseReporter::to(&mut sink)
    } else {
        PhaseReporter::none()
    };
    let sc = session_claims_reporting(args.session, args.exec_db, args.repo, &mut phases)?;

    // §11's cross-session query needs them in the store, not only on screen.
    // Idempotent on the derived id, so re-running `claims` records nothing new.
    if let Ok(repo_id) = sc
        .ledger
        .repo(&sc.repo.to_string_lossy(), Some(&sc.repo.to_string_lossy()))
    {
        for p in &sc.extraction.promises {
            let _ = sc.ledger.insert_promise(
                &repo_id,
                &thruline_ledger::PromiseRow {
                    id: p.id.clone(),
                    session_id: p.session_id.clone(),
                    text: p.text.clone(),
                    kind: serde_json::to_value(p.kind)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| "deferred".to_string()),
                    status: "open".to_string(),
                    opened_event_id: p.opened_event_id.clone(),
                },
            );
        }
    }

    let write_started = Instant::now();
    let code = emit(
        &sc.session,
        &sc.extraction,
        &sc.verdicts,
        &sc.view,
        &sc.repo,
        args.json,
    )?;
    let findings: usize = sc.verdicts.iter().map(|(_, v)| v.findings.len()).sum();
    phases.report(
        "write_up",
        findings as u64,
        format!("{findings} findings"),
        write_started.elapsed(),
    );
    Ok(code)
}

/// One field of recorded text, masked on its way out of this command.
///
/// **Interface request 11.** Claim text was masked only where a producer
/// captured it. `fold` does not re-mask the final assistant message and
/// `/api/claims` serves this command's stdout byte for byte, so a record
/// written by a producer that did not mask reached the screen raw. The mask
/// runs at the serving edge, which is the one place every consumer of the body
/// passes through: `claims --json`, the route, and the panel that quotes it.
///
/// A **Redactor**, not a Gate (`redact::MASK_CALL_SITES`): the masked string is
/// kept, and nothing is refused or judged from whether it fired.
///
/// **The extractor's input is deliberately not masked, and this is the boundary
/// that keeps that true.** What arrives here is a claim or a promise that has
/// already been extracted. The sentence the extractor reads is the `stop`
/// event's own text, and masking THAT would change which claims exist and where
/// their spans fall — `Claim.span.offset` indexes the recorded text, and
/// `derive_id` hashes that offset into the claim id. So this is a display
/// transform and nothing else, which is why it lives in a function of its own
/// rather than inline in `emit`.
fn served_text(text: &str) -> String {
    let (masked, _record) = thruline_core::redact::mask_recorded(text);
    masked
}

fn emit(
    session: &str,
    extraction: &Extraction,
    verdicts: &[(usize, VerdictRecord)],
    view: &SessionView,
    repo: &Path,
    json: bool,
) -> Result<ExitCode> {
    let by_index: BTreeMap<usize, &VerdictRecord> = verdicts.iter().map(|(i, v)| (*i, v)).collect();
    let unsupported = verdicts
        .iter()
        .filter(|(_, v)| v.verdict == Verdict::Unsupported)
        .count();

    if json {
        let claims: Vec<Value> = extraction
            .claims
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let v = by_index.get(&i);
                let mut claim = serde_json::json!({
                    "id": c.id,
                    "kind": c.kind.as_str(),
                    "text": served_text(&c.text),
                    "span": { "event_id": c.span.event_id, "offset": [c.span.offset.0, c.span.offset.1] },
                    "scope": serde_json::to_value(c.args.scope).unwrap_or(Value::Null),
                    "extractor": c.extractor.as_str(),
                    "verdict": v.map(|v| v.verdict.as_str()),
                    "verifier": v.map(|v| v.verifier.clone()),
                    "evidence": v.map(|v| v.evidence.clone()).unwrap_or_default(),
                    "reasons": v.map(|v| v.reasons.clone()).unwrap_or_default(),
                    "warnings": v.map(|v| v.warnings.clone()).unwrap_or_default(),
                    "binding": v.and_then(|v| v.binding.clone()),
                    "findings": v.map(|v| v.findings.clone()).unwrap_or_default(),
                });
                // T8 — the binding's typed cause, lifted onto the claim so a
                // reader asks one field instead of the binding's sentence.
                // Present only when the binding could not be made.
                if let Some(cause) = v.and_then(|v| unobservable_cause(v.binding.as_ref())) {
                    claim["unobservable_cause"] = Value::String(cause.to_string());
                }
                claim
            })
            .collect();
        let promises: Vec<Value> = extraction
            .promises
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "text": served_text(&p.text),
                    "kind": serde_json::to_value(p.kind).unwrap_or(Value::Null),
                    "status": serde_json::to_value(p.status).unwrap_or(Value::Null),
                    "opened_event_id": p.opened_event_id,
                })
            })
            .collect();
        crate::json_out::print(
            &crate::json_out::CLAIMS,
            serde_json::json!({
                "session": session,
                "final_tree_hash": view.final_tree_hash(),
                "checks": view.checks.len(),
                "claims": claims,
                "promises": promises,
                "negated": extraction.negated.iter().map(|(t, k)| serde_json::json!({
                    "text": t, "would_have_been": k.as_str()
                })).collect::<Vec<_>>(),
                // The view's own flag: the one that capped every claim above at
                // `unverifiable` (§13), from the ledger row, a snapshot taken
                // under a content filter, or a lifecycle gap. HQ is handed no
                // receipt, so without it an unverifiable claim in a degraded
                // session could only read as Recorded, never as Fog. It is
                // not the receipt's flag: that one also counts content filters
                // in scope when the receipt is cut.
                "capture_degraded": view.capture_degraded,
            }),
        )?;
    } else {
        println!("session {session}");
        // F4: the vocabulary that decided what follows, named where the reader
        // is. Silent for the built-in table.
        if let Some(said) = table_disclosure(repo) {
            println!("  {said}");
        }
        if extraction.claims.is_empty() {
            println!("  no claims extracted");
        }
        for (i, claim) in extraction.claims.iter().enumerate() {
            let v = by_index.get(&i);
            let verdict = v.map(|v| v.verdict.as_str()).unwrap_or("unjudged");
            println!("  [{verdict}] {} — {:?}", claim.kind.as_str(), claim.text);
            if let Some(v) = v {
                for reason in &v.reasons {
                    println!("      · {reason}");
                }
                for f in &v.findings {
                    println!("      ! {:?} {} — {}", f.severity, f.detector, f.detail);
                }
            }
        }
        for p in &extraction.promises {
            println!("  [open promise] {:?}", p.text);
        }
        for (text, kind) in &extraction.negated {
            println!(
                "  [not a claim] {text:?} — reads as a denial of `{}`",
                kind.as_str()
            );
        }
    }
    // Exit 3 is §14's policy-failure code, so CI can tell a refuted claim from
    // a broken command.
    Ok(if unsupported > 0 {
        ExitCode::from(3)
    } else {
        ExitCode::SUCCESS
    })
}

pub(crate) fn last_session(ledger: &Ledger) -> Result<Option<String>> {
    let mut last: Option<String> = None;
    ledger.for_each_session_id(|s| {
        last = Some(s.to_string());
        Ok(())
    })?;
    Ok(last)
}

/// D18 / F7 — is this session's lifecycle shown complete by its own events?
///
/// # The defect
///
/// Track Q (D18, CRITICAL) and Track R (F7) filed the same hole from opposite
/// sides. A session whose `Stop` hook never fired — a crashed agent, a killed
/// terminal, a hook over its timeout, or an agent that neutered the hook after
/// the run — produced this, measured with the `1a52d60` binary:
///
/// ```text
/// $ thruline explain --last      → "no_stop_recorded — the Stop hook never fired"  (exit 0)
/// $ thruline receipt --out r.json                                                  (exit 0)
/// $ thruline verify-receipt r.json → "Verified."  all 10 ok, 0 verdicts             (exit 0)
/// $ thruline policy              → "policy … — success"                             (exit 0)
/// ```
///
/// The receipt contained no occurrence of `stop`, `truncat` or `incomplete`,
/// and said *"capture was not degraded"* — true in the sense the field meant
/// (no content transforms) and false in the sense a reader takes it. Q's
/// sentence is the one that matters: *"it needs no malice."*
///
/// # The rule, and why it is this shape
///
/// **Where the lifecycle cannot be shown complete, degrade.** A terminal hook
/// must be recorded at or after the last evidence-bearing event. Not merely
/// *present*: R's F7 attack neuters the Stop hook and keeps running through the
/// shim, so a `stop` that precedes the last `exec` is a session that carried on
/// after the last thing the recorder saw. *A hook fired* and *the hook fired
/// last* are different facts and only the second one is worth anything.
///
/// A session with no evidence-bearing events is **not** degraded by this rule.
/// There is nothing for the record to be incomplete about, and flagging it
/// would put a warning on every empty session in the ledger.
///
/// # The false negative, taken deliberately
///
/// **A live session has no terminal hook yet, by definition, so it degrades.**
/// That is not a bug in the rule, it is the rule: the operator's standing
/// decision rule is to take the option that can only move a verdict DOWN, and
/// a live session genuinely does have incomplete capture. Coordinator, under
/// the standing authority: *"a live session being capped at neutral is a false
/// negative that annoys but never overstates."*
///
/// The place it is most likely to annoy is the A200 speaker hooks, which run
/// inside a live session by construction. If it proves too noisy there, that is
/// recorded in `docs/STATUS.md` for the operator to loosen — with a
/// measurement, not with a guess about which sessions are live.
fn lifecycle_gap(events: &[thruline_core::spec::event::Event]) -> Option<String> {
    // Evidence-bearing kinds: the events a claim could be ABOUT. A terminal
    // hook after all of these means the recorder outlived the work.
    const ACTIVITY: &[&str] = &[
        "exec",
        "fs_change",
        "tool_call",
        "tool_result",
        "snapshot",
        "user_prompt",
    ];
    // §3.1's two terminal lifecycle kinds. Every harness maps its own spelling
    // onto them before this point — `Stop`/`SessionEnd` in Claude Code and
    // Codex, `AfterAgent`/`SessionEnd` in Gemini — so this list is the
    // vocabulary and not one harness's names.
    const TERMINAL: &[&str] = &["stop", "session_end"];

    let seq_of = |kinds: &[&str]| -> Option<u64> {
        events
            .iter()
            .filter(|e| kinds.contains(&e.kind().as_str().as_str()))
            .map(|e| e.seq())
            .max()
    };

    let last_activity = seq_of(ACTIVITY)?;
    match seq_of(TERMINAL) {
        Some(t) if t >= last_activity => None,
        Some(t) => Some(format!(
            "hook_events_missing: the last lifecycle hook was recorded at event {t}, and              the session went on to event {last_activity} — so the record ends before the              work does and no claim in it can be shown to be about a finished session"
        )),
        None => Some(format!(
            "hook_events_missing: no `stop` or `session_end` was ever recorded, and the              session reached event {last_activity} — the capture ended without saying so,              which is what a crashed agent, a killed terminal, a hook over its timeout and              a live session all look like from here"
        )),
    }
}

/// Map a session's events and its checks onto the views §6 reads.
pub(crate) fn session_view(
    session: &str,
    events: &[thruline_core::spec::event::Event],
    ledger: &Ledger,
    repo: &Path,
) -> Result<SessionView> {
    session_view_reporting(session, events, ledger, repo, &mut PhaseReporter::none())
}

/// [`session_view`], reporting `match_proof` and `workspace_since` as they
/// complete.
///
/// The event walk (snapshots and edits) is the workspace half and the check
/// rows are the proof half; each is timed separately and both are reported
/// only once the check rows are in, because a check's
/// `paths_changed_since_final` is the workspace fact the second phase quotes.
pub(crate) fn session_view_reporting(
    session: &str,
    events: &[thruline_core::spec::event::Event],
    ledger: &Ledger,
    repo: &Path,
    phases: &mut PhaseReporter<'_>,
) -> Result<SessionView> {
    let workspace_started = Instant::now();
    let mut view = SessionView {
        session_id: session.to_string(),
        // Read, not defaulted. `unwrap_or(false)` turned a failed read of the
        // store into "capture is fine", which is §13's consequence being
        // decided by an error path.
        capture_degraded: ledger
            .capture_degraded(session)
            .with_context(|| format!("reading the capture state of session {session}"))?,
        // Read too, not left `None`. The column has always held the writer's
        // own reason — an imported rollout says why it is degraded — and
        // nothing read it, so every degraded session downstream reported the
        // fact with no cause.
        degraded_reason: ledger
            .degraded_reason(session)
            .with_context(|| format!("reading the capture reason of session {session}"))?,
        ..SessionView::default()
    };
    let mut event_seq_by_id: BTreeMap<String, u64> = BTreeMap::new();
    for e in events {
        event_seq_by_id.insert(e.id().to_string(), e.seq());
        match e.kind().as_str().as_str() {
            "snapshot" => {
                if let Some(hash) = e.payload().get("tree_hash").and_then(Value::as_str) {
                    view.snapshots.push(SnapshotView {
                        seq: e.seq(),
                        tree_hash: hash.to_string(),
                    });
                    if view.head_commit.is_none() {
                        view.head_commit = e
                            .payload()
                            .get("head_commit")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                    }
                    // §13/C.7: a snapshot taken under a content filter cannot
                    // be trusted to name the bytes on disk.
                    if e.payload()
                        .get("capture_degraded")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        view.capture_degraded = true;
                        view.degraded_reason = e
                            .payload()
                            .get("content_filters")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                    }
                }
            }
            "fs_change" => {
                if let Some(path) = e.payload().get("path").and_then(Value::as_str) {
                    view.edits.push(EditView {
                        seq: e.seq(),
                        path: path.to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    // D18 / F7. Applied AFTER the event walk so it sees the whole session, and
    // it never clears a degradation another producer set — `capture_degraded`
    // is a floor here, not an assignment. An imported session is already
    // degraded for its own reason, and that reason is the one worth keeping.
    if let Some(gap) = lifecycle_gap(events) {
        if !view.capture_degraded {
            view.capture_degraded = true;
            view.degraded_reason = Some(gap);
        }
    }

    let workspace_walk = workspace_started.elapsed();
    let proof_started = Instant::now();
    let final_tree = view.final_tree_hash().map(str::to_string);
    let mut workspace_unreadable = false;
    for row in ledger.checks_for_session(session)? {
        let parsed: Value = serde_json::from_str(&row.parsed).unwrap_or(Value::Null);
        // Two producers, two records of the same fact — and neither of them is
        // a sentinel.
        //
        // A **captured** check is derived from an `exec` event, and that
        // event's `seq` is its position. A **re-executed** check (§7) has no
        // `exec` event by design: writing one into the append-only chain ate
        // hook deliveries, so `reexec` records its evidence on the `Check` row
        // instead. Its position comes from the row too — the chain's own
        // record of when the tree it ran against first existed.
        //
        // The miss used to yield `u64::MAX`, which sorted every re-executed
        // check after every edit and made §6.1 rule 2's observed half skip the
        // entire edit record.
        let seq = event_seq_by_id
            .get(&row.exec_event_id)
            .copied()
            .or_else(|| view.first_snapshot_seq_of(&row.input_tree_hash));
        // §6.1 rule 2's tree half. `None` when it cannot be computed, and the
        // binder REFUSES on `None` — a tree it cannot read is not a tree that
        // did not change. T8: that refusal is also the workspace phase's —
        // the step did not compute what it exists to compute, and the Read
        // must not call it done.
        let paths_changed_since_final = match (&final_tree, row.input_tree_hash.is_empty()) {
            (Some(final_tree), false) if *final_tree != row.input_tree_hash => {
                match changed_paths(repo, &row.input_tree_hash, final_tree) {
                    Ok(paths) => Some(paths),
                    Err(WorkspaceUnreadable) => {
                        workspace_unreadable = true;
                        None
                    }
                }
            }
            _ => None,
        };
        view.checks.push(CheckView {
            id: row.id,
            seq,
            category: row.category.clone(),
            bypass: parsed
                .get("bypass")
                .and_then(Value::as_str)
                .map(str::to_string),
            runner: row.runner,
            argv: argv_of(events, &row.exec_event_id, &parsed),
            exit_code: row.exit_code,
            input_tree_hash: row.input_tree_hash,
            paths_changed_since_final,
            stdio: stdio_of(events, &row.exec_event_id, &parsed),
            // §7 provenance. Read from the row's own `reexec` block — the same
            // block `thruline receipt` derives `reexecuted` from, so a row
            // cannot be a re-execution for one reader and a capture for
            // another. Without it the two are indistinguishable here, and a
            // re-executed row (stamped by `reexec` with the final tree hash)
            // outranked the superseded original it re-ran.
            reexecution: reexecution_of(&parsed),
            parsed: parsed
                .get("tests_total")
                .and_then(Value::as_u64)
                .map(|total| ParsedView {
                    tests_total: total,
                    passed: parsed.get("passed").and_then(Value::as_u64).unwrap_or(0),
                    failed: parsed.get("failed").and_then(Value::as_u64).unwrap_or(0),
                    skipped: parsed.get("skipped").and_then(Value::as_u64).unwrap_or(0),
                    xfail: parsed.get("xfail").and_then(Value::as_u64).unwrap_or(0),
                }),
        });
    }
    phases.report(
        "match_proof",
        view.checks.len() as u64,
        match view.checks.len() {
            0 => "no check cited".to_string(),
            n => format!("{n} checks cited"),
        },
        proof_started.elapsed(),
    );
    let mut workspace = format!(
        "{} edits · {} trees",
        view.edits.len(),
        view.snapshots.len()
    );
    // A count, not the path: the paths come from `git diff-tree`, which no
    // capture mask has seen (see [`Phase`]). Same check as before — the first
    // cited check with any path changed since its tree.
    if let Some(n) = view
        .checks
        .iter()
        .filter_map(|c| c.paths_changed_since_final.as_ref())
        .map(Vec::len)
        .find(|n| *n > 0)
    {
        workspace.push_str(&format!(
            " · changed after the last check: {n} path{}",
            if n == 1 { "" } else { "s" }
        ));
    }
    // Reported after `match_proof` (the required order) with the walk's own
    // measured time, taken before the check rows were read. T8: when a cited
    // check's tree differs from the final one and git could not name the
    // paths, the step is reported as failed with a typed slug — the Read then
    // withholds what needs it instead of calling it done.
    phases.report_outcome(
        "workspace_since",
        view.snapshots.len() as u64,
        workspace,
        workspace_walk,
        workspace_unreadable.then_some(WORKSPACE_UNREADABLE),
    );
    Ok(view)
}

/// §7 provenance: was this row produced by `thruline reexec`, and what did it
/// conclude?
///
/// The `reexec` block is written by `thruline_reexec::parsed_block` and is the
/// **only** thing that distinguishes a re-executed row from a captured one — a
/// re-execution writes no `exec` event by design, so there is nothing else to
/// ask. `thruline receipt` derives `reexecuted` from the same block, so the two
/// readers cannot disagree about which rows are re-executions.
///
/// `None` for every captured check, which is what makes §6.1 rule 1 keep
/// meaning "this ran on the final tree" rather than "someone wrote the final
/// tree onto this row".
fn reexecution_of(parsed: &Value) -> Option<Reexecution> {
    let block = parsed.get("reexec")?;
    Some(Reexecution {
        of_check: block
            .get("of_check")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        comparison: block
            .get("comparison")
            .and_then(|c| c.get("outcome"))
            .and_then(Value::as_str)
            .map(str::to_string),
        image_digest: block
            .get("image_digest")
            .and_then(Value::as_str)
            .unwrap_or("(no image digest recorded)")
            .to_string(),
    })
}

/// How a check's child was connected, from whichever producer recorded it.
///
/// The M0.8 recorder writes `stdio` on every `exec` event and §7's container
/// writes it into the `reexec` block, for one reason: a TTY changes the bytes,
/// and §7's comparison must be able to tell "the capture shape differed" from
/// "the output disagreed". Reading it here is what makes that branch reachable
/// — the caller used to assert `"pipe"` for the agent's side, which made both
/// sides equal by construction and the branch dead.
fn stdio_of(
    events: &[thruline_core::spec::event::Event],
    exec_event_id: &str,
    parsed: &Value,
) -> Option<String> {
    if let Some(mode) = events
        .iter()
        .find(|e| e.id().as_str() == exec_event_id)
        .and_then(|e| e.payload().get("stdio"))
        .and_then(Value::as_str)
    {
        return Some(mode.to_string());
    }
    parsed
        .get("reexec")
        .and_then(|r| r.get("stdio"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The command behind a check, from whichever producer recorded it.
///
/// A **captured** check's argv is on its `exec` event. A **re-executed** one's
/// is in the `reexec` block of its `parsed` column, beside the container image
/// digest §8.1 keeps there — `reexec` writes no `exec` event, so the event
/// lookup misses by design.
///
/// `None` when neither holds one. It used to be `Vec::new()`, and an empty
/// argv carries no narrowing flag, so §6.3's narrowing detector reported
/// nothing about a `pytest -k` re-execution — the strongest evidence class the
/// product produces, presenting as a clean full-suite pass.
fn argv_of(
    events: &[thruline_core::spec::event::Event],
    exec_event_id: &str,
    parsed: &Value,
) -> Option<Vec<String>> {
    let strings = |v: &Value| -> Option<Vec<String>> {
        let array = v.as_array()?;
        Some(
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        )
    };
    if let Some(argv) = events
        .iter()
        .find(|e| e.id().as_str() == exec_event_id)
        .and_then(|e| e.payload().get("argv"))
        .and_then(strings)
    {
        return Some(argv);
    }
    parsed
        .get("reexec")
        .and_then(|r| r.get("argv"))
        .and_then(strings)
}

/// §6.7 over every changed test file, base tree versus final tree.
///
/// The blobs are read with `git cat-file`, **never with `git diff`**: C.7 row 12
/// records that a `textconv` or diff driver makes `git diff` lie, so a detector
/// that reads diff output is reading whatever the repository configured it to
/// see.
pub(crate) fn tamper_findings(
    repo: &Path,
    view: &SessionView,
    _events: &[thruline_core::spec::event::Event],
    changed_symbols: &[String],
) -> Vec<thruline_verify::verdict::Finding> {
    let mut out = Vec::new();
    // The base tree is the session's `head_commit`, not its first snapshot.
    //
    // A fold-derived snapshot is taken when the FOLD runs, not at the boundary
    // whose `seq` it carries, so every one in a session holds the same tree —
    // using the first as "before" would compare the final state against itself
    // and find nothing, which is exactly what it did. `head_commit` is a state
    // git recorded before the session touched anything.
    let Some(head) = view.final_tree_hash().map(str::to_string) else {
        return out;
    };
    let base = match view
        .head_commit
        .as_deref()
        .and_then(|c| commit_tree(repo, c))
    {
        Some(tree) => tree,
        None => {
            out.push(thruline_verify::verdict::Finding::new(
                thruline_verify::verdict::Severity::Warn,
                "tamper_unchecked",
                "the session has no committed base state to diff against, so §6.7 \
                 could not run — absence of findings here is not evidence",
            ));
            return out;
        }
    };
    if base == head {
        return out;
    }
    let mut seen: Vec<String> = Vec::new();
    for edit in &view.edits {
        if !thruline_verify::binding::is_test_path(&edit.path) {
            continue;
        }
        let rel = relative_to(repo, &edit.path);
        if seen.contains(&rel) {
            continue;
        }
        seen.push(rel.clone());
        let Some(language) = Language::from_path(&rel) else {
            out.push(
                thruline_verify::verdict::Finding::new(
                    thruline_verify::verdict::Severity::Warn,
                    "tamper_unchecked",
                    format!(
                        "{rel} changed and this build has no grammar for it, so §6.7 \
                         could not look at it — absence of findings here is not evidence"
                    ),
                )
                .at(&rel),
            );
            continue;
        };
        let (Some(before), Some(after)) = (blob(repo, &base, &rel), blob(repo, &head, &rel)) else {
            continue;
        };
        match thruline_verify::tamper::diff_test_file_with_symbols(
            &rel,
            &before,
            &after,
            language,
            changed_symbols,
        ) {
            Ok(findings) => out.extend(findings),
            Err(e) => out.push(thruline_verify::verdict::Finding::new(
                thruline_verify::verdict::Severity::Warn,
                "tamper_unchecked",
                format!("{rel}: {e}"),
            )),
        }
    }
    out
}

/// Extract a session's claims and judge each one.
///
/// **The single place claims meet verifiers.** `thruline claims`,
/// `thruline explain` and `thruline receipt` all come through here, so a
/// receipt can never disagree with what the CLI printed — which would be the
/// product contradicting its own evidence.
pub(crate) fn judge(
    session: &str,
    events: &[thruline_core::spec::event::Event],
    view: &SessionView,
    repo: &Path,
) -> Result<Vec<(thruline_claims::Claim, VerdictRecord)>> {
    judge_reporting(session, events, view, repo, &mut PhaseReporter::none())
}

/// [`judge`], reporting `run_verifiers` once every claim has its verdict.
pub(crate) fn judge_reporting(
    session: &str,
    events: &[thruline_core::spec::event::Event],
    view: &SessionView,
    repo: &Path,
    phases: &mut PhaseReporter<'_>,
) -> Result<Vec<(thruline_claims::Claim, VerdictRecord)>> {
    let verifiers_started = Instant::now();
    let (extraction, table) = extraction_for(session, events, repo)?;
    let ev = diff_evidence_inputs(repo, view);
    let changed_symbols: Vec<String> = ev.api_changes.iter().map(|(_, s)| s.clone()).collect();
    let tamper = tamper_findings(repo, view, events, &changed_symbols);
    let test_side_failed = tamper.iter().any(|f| f.is_fail());
    let special = thruline_verify::special_casing::escalate_with_test_findings(
        ev.special_casing.clone(),
        test_side_failed,
    );
    let source_changed = view
        .edits
        .iter()
        .any(|e| !thruline_verify::binding::is_test_path(&e.path));
    let test_files_changed: Vec<String> = view
        .edits
        .iter()
        .filter(|e| thruline_verify::binding::is_test_path(&e.path))
        .map(|e| e.path.clone())
        .collect();

    let mut out = Vec::new();
    for claim in &extraction.claims {
        let v = claim.verifiable();
        let scope_all = v.args.scope == thruline_claims::Scope::All;
        let record = match v.kind {
            ClaimKind::TestsPass => {
                let mut findings = tamper.clone();
                findings.extend(special.iter().cloned());
                tests_pass::verify(
                    &v.id,
                    view,
                    &Inputs {
                        scope_is_all: scope_all,
                        tamper_findings: findings,
                        source_changed,
                        test_files_changed: test_files_changed.clone(),
                        coverage: ev.coverage.clone(),
                    },
                )
            }
            ClaimKind::BuildOk => tool_checks::verify(&v.id, "build", view, scope_all),
            ClaimKind::LintOk => tool_checks::verify(&v.id, "lint", view, scope_all),
            ClaimKind::TypecheckOk => tool_checks::verify(&v.id, "typecheck", view, scope_all),
            ClaimKind::BehaviorUnchanged => behavior_unchanged::verify(
                &v.id,
                view,
                &behavior_unchanged::Inputs {
                    test_files_changed: test_files_changed.clone(),
                    manifests_changed: ev.manifests_changed.clone(),
                    api_changes: ev.api_changes.clone(),
                    api_diff_available: ev.api_diff_available,
                },
            ),
            ClaimKind::MigrationAdded => de::migration_added(&v.id, &ev.changed_paths),
            ClaimKind::DocsUpdated => de::docs_updated(&v.id, &ev.changed_paths),
            ClaimKind::DependencyAdded => de::dependency_added(&v.id, &ev.changed_paths),
            ClaimKind::SecretsAbsent => de::secrets_absent(&v.id, &ev.secrets),
            ClaimKind::FileChanged | ClaimKind::DidX => {
                de::file_changed(&v.id, &v.args.names, &ev.changed_paths)
            }
            ClaimKind::TaskComplete => {
                VerdictRecord::new(&v.id, "task_complete@0", Verdict::Unverifiable).reason(
                    "no verifier for `task_complete`: it would need the harness's \
                     TaskCreated/TaskCompleted events, which this build does not \
                     subscribe to (§4.1, deferred to M4)",
                )
            }
        };
        out.push((claim.clone(), record));
    }

    // **F5's floor.** A session that changed files and made no claim this build
    // recognises is not a session with nothing to prove — it is a session whose
    // completion assertion was written in English the table does not contain.
    // Left alone it produced `success` and a green badge, indistinguishable
    // from a proven-good run. See `thruline_claims::unclaimed_completion` for
    // why the fix is not a longer phrase list.
    if let Some((anchor, message)) = final_message(events) {
        if let Some(claim) = thruline_claims::unclaimed_completion(
            session,
            &anchor,
            &message,
            &extraction,
            ev.changed_paths.len(),
        ) {
            let record = VerdictRecord::new(&claim.id, UNCLAIMED_VERIFIER, Verdict::Unverifiable)
                .reason(format!(
                    "this session changed {} file(s) and stopped without a completion claim \
                     this build recognises. Handing the work back is itself a completion \
                     assertion, and nothing in the record proves it: no claim was extracted, \
                     so no verifier ran. The absence of a claim is not the absence of a \
                     problem, and this session may not read as verified.",
                    ev.changed_paths.len()
                ));
            out.push((claim, record));
        }
    }

    // **F4's disclosure.** A repository table that chose the words a reader was
    // shown appeared nowhere in the signed payload. It rides on every verdict,
    // so a receipt cannot carry a judgement without carrying the vocabulary
    // that produced it.
    if let Some(said) = table.disclosure() {
        for (_, record) in out.iter_mut() {
            record.warnings.push(said.clone());
        }
    }
    let verifiers: std::collections::BTreeSet<&str> =
        out.iter().map(|(_, v)| v.verifier.as_str()).collect();
    phases.report(
        "run_verifiers",
        out.len() as u64,
        verifiers.into_iter().collect::<Vec<_>>().join(" · "),
        verifiers_started.elapsed(),
    );
    Ok(out)
}

/// `unclaimed_completion@1` — the named check behind F5's floor. It is a
/// distinct verifier string rather than `task_complete@0` so a reader can see
/// from the receipt alone that no phrase matched.
pub(crate) const UNCLAIMED_VERIFIER: &str = "unclaimed_completion@1";

/// The last `stop` event's id and message.
fn final_message(events: &[thruline_core::spec::event::Event]) -> Option<(String, String)> {
    events
        .iter()
        .rev()
        .find(|e| e.kind().as_str() == "stop")
        .and_then(|e| {
            let text = e.payload().get("last_assistant_message")?.as_str()?;
            Some((e.id().to_string(), text.to_string()))
        })
}

/// F4's one-line disclosure for this repository, or `None` for the built-in
/// table. A load error is reported rather than swallowed: a table that did not
/// load is not the built-in table.
pub(crate) fn table_disclosure(repo: &Path) -> Option<String> {
    match Table::load(repo) {
        Ok(t) => t.disclosure(),
        Err(e) => Some(format!("claim table DID NOT LOAD: {e}")),
    }
}

/// The claims and promises a session's final message carries.
pub(crate) fn extraction_for(
    session: &str,
    events: &[thruline_core::spec::event::Event],
    repo: &Path,
) -> Result<(Extraction, Table)> {
    let table = Table::load(repo).context("loading the claim pattern table")?;
    let sources: Vec<Source> = events
        .iter()
        .filter(|e| e.kind().as_str() == "stop")
        .filter_map(|e| {
            Some(Source {
                event_id: e.id().to_string(),
                session_id: session.to_string(),
                text: e
                    .payload()
                    .get("last_assistant_message")?
                    .as_str()?
                    .to_string(),
            })
        })
        .collect();
    Ok((extract(&sources, &table), table))
}

/// Everything §6.5, §6.6 and §6.8 read about the session's diff.
pub(crate) struct DiffEvidence {
    pub changed_paths: Vec<String>,
    pub coverage: Option<thruline_verify::coverage::Coverage>,
    pub manifests_changed: Vec<String>,
    pub api_changes: Vec<(String, String)>,
    pub api_diff_available: bool,
    pub secrets: Vec<(String, String)>,
    pub special_casing: Vec<thruline_verify::verdict::Finding>,
}

const MANIFEST_NAMES: &[&str] = &[
    "cargo.toml",
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
    "gemfile",
    "composer.json",
    "pom.xml",
    "build.gradle",
];

/// Read the session's base and final trees and derive the §6.5/§6.6/§6.8 inputs.
///
/// Base is `head_commit`'s tree — a state git recorded — for the same reason
/// §6.7 uses it: every fold-derived snapshot in a session carries the same
/// hash, so the first snapshot is not "before".
pub(crate) fn diff_evidence_inputs(repo: &Path, view: &SessionView) -> DiffEvidence {
    let mut out = DiffEvidence {
        changed_paths: Vec::new(),
        coverage: None,
        manifests_changed: Vec::new(),
        api_changes: Vec::new(),
        api_diff_available: false,
        secrets: Vec::new(),
        special_casing: Vec::new(),
    };
    let (Some(base), Some(head)) = (
        view.head_commit
            .as_deref()
            .and_then(|c| commit_tree(repo, c)),
        view.final_tree_hash().map(str::to_string),
    ) else {
        return out;
    };
    let Ok(changed) = changed_paths(repo, &base, &head) else {
        return out;
    };
    out.api_diff_available = true;
    out.manifests_changed = changed
        .iter()
        .filter(|p| {
            let f = p.rsplit('/').next().unwrap_or(p).to_ascii_lowercase();
            MANIFEST_NAMES.contains(&f.as_str()) || f.ends_with(".lock")
        })
        .cloned()
        .collect();

    // Test-file literals, for §6.8's "branch keyed on a value the tests use".
    let mut test_literals: std::collections::BTreeSet<String> = Default::default();
    for path in &changed {
        if thruline_verify::binding::is_test_path(path) {
            if let Some(text) = blob(repo, &head, path) {
                test_literals.extend(thruline_verify::special_casing::literals_in(&text));
            }
        }
    }

    for path in &changed {
        let before = blob(repo, &base, path);
        let after = blob(repo, &head, path);
        // §6.6 `secrets_absent`: the scanner over the ADDED text only.
        if let Some(after_text) = &after {
            let added_only: String = after_text
                .lines()
                .filter(|l| before.as_deref().map(|b| !b.contains(*l)).unwrap_or(true))
                .collect::<Vec<_>>()
                .join("\n");
            // **A Gate, and an inverted one.** `redact::MASK_CALL_SITES`
            // classifies this: the two existing gates REFUSE a value, where a
            // false positive costs nothing and a false negative costs
            // everything, so they use the loose prefix threshold. This one is
            // the other way round — firing makes `secrets_absent`
            // `unsupported`, so a false positive wrongly accuses an honest
            // claim.
            //
            // So it is the CONJUNCTION: the masker fired (a plausible body)
            // *and* a known credential prefix is present. That excludes
            // entropy-only and context-only detections — `api_key=<opaque>` in
            // a diff will not refute the claim, and does not need to, because
            // `secrets_absent`'s ceiling is `unverifiable` either way
            // (CONFLICT-8). Nothing that could have been `supported` is lost.
            let (_, record) = thruline_core::redact::mask_recorded(&added_only);
            let corroborated = thruline_core::redact::resembles_known_credential(&added_only);
            for family in &record.families {
                // A **structural** family is self-identifying and satisfies the
                // gate alone. Requiring the prefix test for all of them dropped
                // the single most serious true positive this claim exists to
                // catch: a committed private key fires `private_key_block` and
                // has no prefix for `resembles_known_credential` to find, so
                // the conjunction left it `unverifiable` rather than accused.
                //
                // Everything else keeps the conjunction — see
                // `redact::STRUCTURAL_FAMILIES` for why the two are not the
                // same question.
                // …and it accuses only when the block would ACCUSE: see
                // `redact::contains_accusable_key_block`. The exemption without
                // that test promoted A209-C12's disclosed over-mask — a genuine
                // private-key label with a placeholder body — into a false
                // accusation, which is the same polarity boundary this gate was
                // built around, crossed in the other direction.
                let structural = thruline_core::redact::STRUCTURAL_FAMILIES
                    .contains(&family.as_str())
                    && thruline_core::redact::contains_accusable_key_block(&added_only);
                if structural || corroborated {
                    out.secrets.push((path.clone(), family.clone()));
                }
            }
        }
        if thruline_verify::binding::is_test_path(path) {
            continue;
        }
        let Some(language) = thruline_verify::tamper::Language::from_path(path) else {
            continue;
        };
        // §6.5's public-interface diff.
        if let (Some(b), Some(a)) = (&before, &after) {
            if let (Ok(was), Ok(now)) = (
                thruline_verify::tamper::public_symbols(b, language),
                thruline_verify::tamper::public_symbols(a, language),
            ) {
                for symbol in was.iter().filter(|s| !now.contains(s)) {
                    out.api_changes.push((path.clone(), symbol.clone()));
                }
                for symbol in now.iter().filter(|s| !was.contains(s)) {
                    out.api_changes.push((path.clone(), symbol.clone()));
                }
            }
            // §6.8's source-side heuristics.
            out.special_casing
                .extend(thruline_verify::special_casing::scan(
                    path,
                    b,
                    a,
                    &test_literals,
                ));
        }
    }
    // §6.3's coverage clause. Absent artifacts leave this `None`, which reads
    // as "not measured" and never as 0%.
    out.coverage = Some(thruline_verify::coverage::changed_line_coverage(
        &changed_lines(repo, &base, &head),
        &coverage_hits(repo),
    ));
    out.changed_paths = changed;
    out
}

/// Line numbers added or modified between two trees, per path.
///
/// `-U0` so the hunk headers name exactly the changed lines and nothing else.
fn changed_lines(
    repo: &Path,
    base: &str,
    head: &str,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<u32>> {
    let mut out: std::collections::BTreeMap<String, std::collections::BTreeSet<u32>> =
        Default::default();
    let Ok(o) = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["diff", "-U0", "--no-textconv", base, head])
        .output()
    else {
        return out;
    };
    if !o.status.success() {
        return out;
    }
    let text = String::from_utf8_lossy(&o.stdout);
    let mut current = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            current = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("@@ ") {
            // `@@ -a,b +c,d @@`
            let Some(plus) = rest.split('+').nth(1) else {
                continue;
            };
            let spec = plus.split(' ').next().unwrap_or("");
            let mut parts = spec.split(',');
            let Some(Ok(start)) = parts.next().map(str::parse::<u32>) else {
                continue;
            };
            let count: u32 = parts.next().and_then(|c| c.parse().ok()).unwrap_or(1);
            if current.is_empty() {
                continue;
            }
            let entry = out.entry(current.clone()).or_default();
            for n in start..start + count {
                entry.insert(n);
            }
        }
    }
    out
}

/// The coverage artifact, if the repository has one.
fn coverage_hits(repo: &Path) -> thruline_verify::coverage::LineHits {
    for (name, lcov) in [
        ("lcov.info", true),
        ("coverage/lcov.info", true),
        ("coverage.xml", false),
        ("cobertura.xml", false),
        ("target/coverage/lcov.info", true),
    ] {
        let path = repo.join(name);
        if !path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        return if lcov {
            thruline_verify::coverage::parse_lcov(&text)
        } else {
            thruline_verify::coverage::parse_cobertura(&text)
        };
    }
    Default::default()
}

/// Paths that differ between two trees.
///
/// `git diff-tree`, with `--no-textconv` because C.7 row 12 records that a
/// `textconv` or diff driver makes `git diff` lie about content. Names only are
/// read, so a driver cannot change the answer — but the flag is passed anyway
/// so a future reader does not have to work that out.
///
/// Used only to CONTRADICT an observed change set, never to enumerate one: a
/// clean filter that hides a write from the tree hides it here too, and that is
/// what the `fs_change` path exists to catch.
///
/// `Err` when git cannot run here or `diff-tree` fails: the paths are
/// **unknown**, never "none changed" (T8).
fn changed_paths(repo: &Path, a: &str, b: &str) -> Result<Vec<String>, WorkspaceUnreadable> {
    let out = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args([
            "diff-tree",
            "-r",
            "--name-only",
            "--no-commit-id",
            "--no-textconv",
            a,
            b,
        ])
        .output()
        .map_err(|_| WorkspaceUnreadable)?;
    if !out.status.success() {
        return Err(WorkspaceUnreadable);
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// The workspace could not say which paths differ between two trees: git is
/// not reachable here, or `diff-tree` refused (a tree object it cannot read).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceUnreadable;

/// The typed slug `workspace_since` fails with, and the one the binder's
/// [`thruline_verify::binding::UnobservableCause::WorkspaceUnreadable`] serializes as.
const WORKSPACE_UNREADABLE: &str = "workspace_unreadable";

/// The typed cause of a binding that could not be made, or `None`.
fn unobservable_cause(binding: Option<&thruline_verify::binding::Binding>) -> Option<&'static str> {
    match binding? {
        thruline_verify::binding::Binding::Unobservable { cause, .. } => cause.map(|c| c.as_str()),
        _ => None,
    }
}

/// The tree a commit points at.
fn commit_tree(repo: &Path, commit: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["rev-parse", &format!("{commit}^{{tree}}")])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

fn relative_to(repo: &Path, path: &str) -> String {
    let p = Path::new(path);
    p.strip_prefix(repo)
        .unwrap_or(p)
        .to_string_lossy()
        .to_string()
}

fn blob(repo: &Path, tree: &str, path: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["cat-file", "blob", &format!("{tree}:{path}")])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

#[cfg(test)]
mod phase_tests {
    use super::*;

    /// An unmeasured phase is `null` on the wire: unknown, never a number.
    #[test]
    fn an_unknown_elapsed_time_is_null_not_a_number() {
        let unknown = Phase {
            phase: "read_record",
            count: 3,
            detail: "d".to_string(),
            elapsed_ms: None,
            failed: None,
        };
        assert_eq!(phase_json(&unknown)["elapsed_ms"], Value::Null);
        let measured = Phase {
            elapsed_ms: Some(7),
            ..unknown.clone()
        };
        assert_eq!(phase_json(&measured)["elapsed_ms"], serde_json::json!(7));
    }

    /// A duration whose milliseconds do not fit is recorded as unknown, and a
    /// measurable one keeps its measurement.
    #[test]
    fn the_reporter_records_an_unrepresentable_duration_as_unknown() {
        let mut seen: Vec<Phase> = Vec::new();
        let mut sink = |p: &Phase| seen.push(p.clone());
        {
            let mut reporter = PhaseReporter::to(&mut sink);
            reporter.report("read_record", 1, "d".to_string(), Duration::MAX);
            reporter.report("find_claims", 1, "d".to_string(), Duration::from_millis(5));
        }
        assert_eq!(seen[0].elapsed_ms, None);
        assert_eq!(seen[1].elapsed_ms, Some(5));
    }
}
