//! `thruline explain` — BUILD-SPEC §10, from the ledger alone.
//!
//! Answers all six questions: what happened, why it stopped, what it forgot,
//! what it promised and did not do, what it claimed versus what it proved, and
//! what is risky.
//!
//! # §6.9 is printed BESIDE the claims table, never inside it
//!
//! Ruling C.9. The honest-failure line comes from `crates/explain`, which
//! `crates/verify` cannot reach — so a sentence can change what a reader is
//! told and can never change a verdict. The two are rendered next to each
//! other on purpose: a reader should see "the agent reported a failure" and
//! "the claim is unsupported" at once, rather than have the first quietly
//! excuse the second.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde_json::Value;
use thruline_claims::{extract, Promise, Source, Table};
use thruline_explain::compaction::{compaction_loss, PreCompaction};
use thruline_explain::narrative::honest_failure;
use thruline_explain::stop::{classify_stop, EventView};
use thruline_explain::timeline::{build as build_timeline, TimelineEvent};
use thruline_ledger::Ledger;
use thruline_verify::verdict::Verdict;

pub struct Args {
    pub session: Option<String>,
    pub last: bool,
    pub full: bool,
    pub json: bool,
    pub md: bool,
    pub html: bool,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
}

/// §10: "terminal (default, colored, ≤60 lines, then `--full`)".
const DEFAULT_LINE_BUDGET: usize = 60;

pub fn run(args: Args) -> Result<ExitCode> {
    let db = match args.exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    if !db.is_file() {
        anyhow::bail!("no execution store at {}", db.display());
    }
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let _ = args.last;
    let session = match args.session {
        Some(s) => s,
        None => crate::claims_cmd::last_session(&ledger)?.context("no session in the store")?,
    };
    let repo = args
        .repo
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let events = ledger.session_events(&session)?;

    // --- 1. what happened --------------------------------------------------
    let timeline = timeline_of(&events);

    // --- 2. why it stopped -------------------------------------------------
    let stop = classify_stop(
        &events
            .iter()
            .map(|e| EventView {
                seq: e.seq(),
                kind: e.kind().as_str(),
                summary: summarize(e),
                stop_reason: e
                    .payload()
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                // `ok == false` is only a failure when something *said* so.
                // Codex fires one `PostToolUse` for both outcomes and its
                // documented payload names neither, so an unstated outcome must
                // not become "the agent's tool failed" in a narrative a person
                // reads — nor "it worked". `tool_outcome` is the only reader of
                // the pair, and its third state is what carries that.
                failed_tool: (e.kind().as_str() == "tool_result"
                    && thruline_capture::fold::tool_outcome(e.payload()).is_stated_failure())
                .then(|| {
                    format!(
                        "{} — {}",
                        e.payload()
                            .get("tool")
                            .and_then(Value::as_str)
                            .unwrap_or("a tool"),
                        e.payload()
                            .get("output_excerpt")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .lines()
                            .next()
                            .unwrap_or("")
                    )
                }),
            })
            .collect::<Vec<_>>(),
    );

    // --- 3. what it forgot -------------------------------------------------
    let pres: Vec<PreCompaction> = events
        .iter()
        .filter(|e| e.kind().as_str() == "compaction_pre")
        .map(|e| PreCompaction {
            seq: e.seq(),
            open_promises: strings(e.payload().get("open_promises")),
            open_tasks: strings(e.payload().get("open_tasks")),
            recent_claims: strings(e.payload().get("recent_claims")),
        })
        .collect();
    let after: Vec<(u64, String)> = events.iter().map(|e| (e.seq(), summarize(e))).collect();
    let loss = compaction_loss(&pres, &after);

    // --- 4 & 5. promises, claims and proof ---------------------------------
    let table = Table::load(&repo)?;
    let final_message = events
        .iter()
        .rev()
        .find(|e| e.kind().as_str() == "stop")
        .and_then(|e| {
            e.payload()
                .get("last_assistant_message")
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .to_string();
    let sources: Vec<Source> = events
        .iter()
        .filter(|e| e.kind().as_str() == "stop")
        .map(|e| Source {
            event_id: e.id().to_string(),
            session_id: session.clone(),
            text: e
                .payload()
                .get("last_assistant_message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
        .collect();
    let extraction = extract(&sources, &table);

    let view = crate::claims_cmd::session_view(&session, &events, &ledger, &repo)?;
    // **`judge` is the single place claims meet verifiers** — its own docblock
    // says `explain` comes through here, and for a long time that sentence was
    // false. This command re-derived its verdicts inline from
    // `extraction.claims`, which skipped F5's floor
    // (`thruline_claims::unclaimed_completion`), the check that fires exactly
    // when no phrase matched and files changed anyway.
    //
    // Measured on the operator's store (2026-09-08, 13 sessions carrying a
    // `stop`): `thruline claims` reported one claim on 7 of them where
    // `thruline explain --json` reported `claims_vs_proof: []`, and the app
    // reads `explain`. So `thruline ui` said "no completion claim was made,
    // so there is nothing to find against" about sessions that changed up to
    // 18 files — a session that read as green because the floor never ran.
    // That is the product contradicting its own evidence, which is the exact
    // failure `judge`'s docblock exists to forbid.
    //
    // `judged` therefore replaces both the extraction-side claim list and the
    // hand-rolled verdicts below. `extraction.promises` is untouched, and so
    // is §6.9's `honest_failure` argument: that asks whether the agent said it
    // finished IN WORDS, which is a question about extracted language and not
    // about a claim this build synthesises.
    let judged = crate::claims_cmd::judge(&session, &events, &view, &repo)?;

    // §6.9, narrative only.
    let honesty = honest_failure(&final_message, !extraction.claims.is_empty());

    if args.html {
        let report = thruline_explain::html::Report {
            session: session.clone(),
            repo: repo.to_string_lossy().to_string(),
            generated: thruline_core::ingest::iso8601_utc_now(),
            timeline: timeline
                .entries
                .iter()
                .map(|e| (e.seq, e.kind.clone(), e.text.clone()))
                .collect(),
            stop_kind: stop.kind.as_str().to_string(),
            stop_summary: stop.summary.clone(),
            last_events: stop.last_events.clone(),
            compaction_note: if loss.compactions == 0 {
                "No compaction happened in this session.".to_string()
            } else if let Some(why) = &loss.unmeasurable {
                why.clone()
            } else if loss.lost.is_empty() {
                "Nothing open before the compaction went unmentioned after it.".to_string()
            } else {
                loss.lost
                    .iter()
                    .map(|l| format!("dropped {}: {}", l.kind, l.what))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
            promises: extraction
                .promises
                .iter()
                .map(|p| ("open".to_string(), p.text.clone()))
                .collect(),
            claims: judged
                .iter()
                .map(|(c, v)| {
                    (
                        v.verdict.as_str().to_string(),
                        c.kind.as_str().to_string(),
                        c.text.clone(),
                        v.reasons.clone(),
                    )
                })
                .collect(),
            findings: judged
                .iter()
                .flat_map(|(_, v)| v.findings.iter())
                .map(|f| {
                    (
                        format!("{:?}", f.severity).to_lowercase(),
                        f.detector.clone(),
                        f.detail.clone(),
                    )
                })
                .collect(),
            risks: timeline
                .risks
                .iter()
                .map(|r| (r.seq, r.text.clone(), r.risk.clone().unwrap_or_default()))
                .collect(),
            narrative: honesty.narrative(),
        };
        print!("{}", thruline_explain::html::render(&report));
    } else if args.json {
        crate::json_out::print(
            &crate::json_out::EXPLAIN,
            serde_json::json!({
                "session": session,
                "what_happened": timeline.entries,
                "why_it_stopped": stop,
                "what_it_forgot": loss,
                "promises": served_promises(&extraction.promises),
                "claims_vs_proof": judged.iter()
                    .map(|(c, v)| serde_json::json!({
                        "kind": c.kind.as_str(), "text": served_text(&c.text),
                        "verdict": v.verdict.as_str(), "reasons": v.reasons,
                        "findings": v.findings,
                    })).collect::<Vec<_>>(),
                "risk_flags": timeline.risks,
                "narrative": honesty.narrative(),
            }),
        )?;
    } else {
        let mut out = String::new();
        let h = |s: &mut String, t: &str| {
            if args.md {
                s.push_str(&format!("\n## {t}\n\n"));
            } else {
                s.push_str(&format!("\n{t}\n"));
            }
        };
        out.push_str(&format!("session {session}\n"));

        h(&mut out, "What happened");
        let budget = if args.full {
            usize::MAX
        } else {
            DEFAULT_LINE_BUDGET / 3
        };
        for e in timeline.entries.iter().take(budget) {
            out.push_str(&format!("  {:>5}  {:<14} {}\n", e.seq, e.kind, e.text));
        }
        if timeline.entries.len() > budget {
            out.push_str(&format!(
                "  … {} more ({} tool calls grouped) — `--full` for all\n",
                timeline.entries.len() - budget,
                timeline.grouped
            ));
        }

        h(&mut out, "Why it stopped");
        out.push_str(&format!("  {} — {}\n", stop.kind.as_str(), stop.summary));
        for e in &stop.last_events {
            out.push_str(&format!("    before it: {e}\n"));
        }

        h(&mut out, "What it forgot");
        if loss.compactions == 0 {
            out.push_str("  no compaction happened in this session\n");
        } else if let Some(why) = &loss.unmeasurable {
            out.push_str(&format!("  {why}\n"));
        } else if loss.lost.is_empty() {
            out.push_str("  nothing open before the compaction went unmentioned after it\n");
        } else {
            for item in &loss.lost {
                out.push_str(&format!("  dropped {}: {}\n", item.kind, item.what));
            }
        }

        h(&mut out, "What it promised and did not do");
        if extraction.promises.is_empty() {
            out.push_str("  no open promises\n");
        }
        for p in &extraction.promises {
            out.push_str(&format!("  open: {}\n", p.text));
        }

        h(&mut out, "What it claimed vs what it proved");
        // F4: which vocabulary decided what follows. Silent for the built-in
        // table, because a disclosure on every session is one nobody reads.
        if let Some(said) = table.disclosure() {
            out.push_str(&format!("  {said}\n"));
        }
        if judged.is_empty() {
            out.push_str("  no completion claims were made\n");
        }
        for (c, v) in judged.iter() {
            out.push_str(&format!(
                "  {:<12} {:<14} {}\n",
                v.verdict.as_str(),
                c.kind.as_str(),
                c.text
            ));
            for r in v
                .reasons
                .iter()
                .take(if args.full { usize::MAX } else { 2 })
            {
                out.push_str(&format!("               · {r}\n"));
            }
        }
        if let Some(n) = honesty.narrative() {
            // Beside the table, never inside it.
            out.push_str(&format!("\n  note: {n}\n"));
        }

        h(&mut out, "Risk flags");
        let findings: Vec<&thruline_verify::verdict::Finding> =
            judged.iter().flat_map(|(_, v)| v.findings.iter()).collect();
        if timeline.risks.is_empty() && findings.is_empty() {
            out.push_str("  none\n");
        }
        for r in &timeline.risks {
            out.push_str(&format!(
                "  destructive  seq {:<5} {}  — {}\n",
                r.seq,
                r.text,
                r.risk.clone().unwrap_or_default()
            ));
        }
        for f in findings {
            out.push_str(&format!(
                "  {:<12} {} — {}\n",
                format!("{:?}", f.severity).to_lowercase(),
                f.detector,
                f.detail
            ));
        }
        print!("{out}");
    }

    Ok(
        if judged
            .iter()
            .any(|(_, v)| v.verdict == Verdict::Unsupported)
        {
            ExitCode::from(3)
        } else {
            ExitCode::SUCCESS
        },
    )
}

/// §10 item 1's timeline, from a session's events.
///
/// Public because `thruline tui` renders the same timeline. Two renderers over
/// two builders would describe the same event differently in two places, and a
/// reader who saw both would have to decide which the product meant.
pub(crate) fn timeline_of(
    events: &[thruline_core::spec::event::Event],
) -> thruline_explain::timeline::Timeline {
    build_timeline(
        &events
            .iter()
            .map(|e| TimelineEvent {
                seq: e.seq(),
                kind: e.kind().as_str(),
                text: summarize(e),
                argv: e
                    .payload()
                    .get("argv")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                command: e
                    .payload()
                    .get("input_redacted")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                // Carried verbatim: a bare harness identifier, no cut, so no
                // truncation site and nothing to register. `summarize` glues
                // this to the argument with a space, which forced every reader
                // to re-split the result; now it does not have to guess.
                tool: e
                    .payload()
                    .get("tool")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                // Through the SAME `truncate` — the registered mask-then-cut of
                // `redact::TRUNCATION_SITES[0]` — at the store's own ceiling
                // rather than a second, invented one: capture already masks and
                // bounds `input_redacted` to 2,000 characters
                // (`capture::fold::compact`), so `INPUT_BUDGET` re-states a bound
                // that exists rather than adding one that needs defending. A new
                // bounding helper here would be a new truncation site and would
                // fail the census until enumerated; reusing `truncate` changes a
                // number, not an ordering.
                input: e
                    .payload()
                    .get("input_redacted")
                    .and_then(Value::as_str)
                    .map(|s| truncate(s, INPUT_BUDGET)),
            })
            .collect::<Vec<_>>(),
    )
}

/// The bound on [`thruline_explain::timeline::Entry::input`].
///
/// Deliberately equal to the capture-time cap in `capture::fold::compact`, so
/// this is not a second ceiling. Anything shorter would cut values the ledger
/// chose to keep, and would have to justify its own number; at 2,000 the field
/// is byte-identical to the stored value for all but the rows capture itself
/// already cut, and `truncate`'s `…` still fires honestly on those.
const INPUT_BUDGET: usize = 2_000;

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| x.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One line describing an event, for the timeline.
fn summarize(e: &thruline_core::spec::event::Event) -> String {
    let p = e.payload();
    let s = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("");
    match e.kind().as_str().as_str() {
        "session_start" => "session started".to_string(),
        "user_prompt" => truncate(s("prompt"), 70),
        "tool_call" => format!("{} {}", s("tool"), truncate(s("input_redacted"), 50)),
        // **The second reader, and it was unguarded.** This rendered every
        // Codex tool result as `ok` in a timeline a person reads, because its
        // `else` branch treated "not stated" and "succeeded" as one thing. The
        // census below now makes a second reader of `ok` impossible; this is
        // what it caught on the day it was written.
        "tool_result" => format!(
            "{} {}",
            s("tool"),
            thruline_capture::fold::tool_outcome(p).label()
        ),
        "exec" => format!(
            "{} → exit {}",
            p.get("argv")
                .and_then(Value::as_array)
                .map(|a| a
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "))
                .unwrap_or_default(),
            p.get("exit_code").and_then(Value::as_i64).unwrap_or(-1)
        ),
        "fs_change" => format!("{} {}", s("op"), s("path")),
        "snapshot" => format!(
            "tree {}{}",
            short_hash(s("tree_hash")),
            if p.get("contemporaneous").and_then(Value::as_bool) == Some(false) {
                " (taken at fold)"
            } else {
                ""
            }
        ),
        "stop" => format!("stopped: {}", truncate(s("last_assistant_message"), 60)),
        "session_end" => "session ended".to_string(),
        "note" => {
            let note = s("note");
            if note.is_empty() {
                "note".to_string()
            } else {
                format!("{note} {}", truncate(s("exec_id"), 12))
            }
        }
        other => other.to_string(),
    }
}

/// Abbreviate a tree hash or digest for display.
///
/// Declared in `redact::NON_RENDERING_TRUNCATIONS`: the input is a 40-character
/// hex object id this product publishes, so no credential can be split by it.
fn short_hash(hex: &str) -> String {
    hex.chars().take(12).collect()
}

/// One line of already-redacted text, bounded for the terminal.
///
/// **Masks, then cuts — locally, in this function.** Everything reaching here
/// came out of the ledger and was redacted at capture, so the mask is a fixpoint
/// and finds nothing. It runs anyway because `redact::TRUNCATION_SITES` verifies
/// the ordering by PROXIMITY inside one function, and a guarantee that holds two
/// crates away is invisible to it. A209-C3 measured what cutting first does: a
/// token past character 105 in a 143-character command reached a sealed artifact
/// because the cut removed enough body for the detector to stop recognising the
/// shape.
///
/// The census caught this site the moment it was written, which is the whole
/// point of having it.
fn truncate(s: &str, n: usize) -> String {
    let one_line: String = s.split('\n').next().unwrap_or("").to_string();
    let (masked, _record) = thruline_core::redact::mask_recorded(&one_line);
    let cut = thruline_core::redact::truncate_respecting_markers(&masked, n);
    if cut.len() < masked.len() {
        return format!("{cut}…");
    }
    cut
}

/// One field of recorded text, masked on its way out of this command.
///
/// **Interface request 16**, which extends request 11 from `/api/claims` to
/// this command's `claims_vs_proof[].text` and `promises[].text`. Both were
/// served exactly as the record held them while the `stop` row in
/// `what_happened` was masked by [`truncate`] — the same body, two different
/// treatments of the same agent's words.
///
/// A **Redactor**, not a Gate (`redact::MASK_CALL_SITES`): the masked string is
/// kept and nothing is refused or judged from whether it fired.
///
/// # This function exists so that `run` does not do the masking
///
/// `run` holds both the text that must be masked and the text that must not.
/// A few lines above the JSON body it builds `Source.text` from the `stop`
/// event's `last_assistant_message` and hands it to `extract`, and masking THAT
/// would change which claims are extracted and where their spans fall — a
/// behaviour change wearing a privacy fix's clothes. A census entry reading
/// `("explain_cmd.rs", "run", …)` would record that the function masks while
/// hiding that one of its two text paths deliberately does not; an entry naming
/// this function says only what is true.
fn served_text(text: &str) -> String {
    let (masked, _record) = thruline_core::redact::mask_recorded(text);
    masked
}

/// `promises[]`, with each promise's text masked by [`served_text`].
///
/// **The `Promise` is cloned and re-serialised, never rebuilt as a `json!`
/// object.** `closed_event_id` and `closed_reason` carry
/// `skip_serializing_if = "Option::is_none"`, so a hand-built object would
/// drop the conditional-field behaviour and change the body's shape for every
/// consumer — a bigger change than the one being made, and a silent one.
fn served_promises(promises: &[Promise]) -> Vec<Promise> {
    promises
        .iter()
        .map(|p| Promise {
            text: served_text(&p.text),
            ..p.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{truncate, INPUT_BUDGET};

    /// A credential straddling the NEW bound must not survive it.
    ///
    /// `Entry::input` widened the timeline's cut from 50 characters to
    /// [`INPUT_BUDGET`], and a wider cut is a new place for A209-C3's failure to
    /// happen: cut first and a key loses enough body for the detector to stop
    /// recognising its shape, so it ships in clear. This pins the ordering at
    /// the bound that actually ships, with the control that proves the case is
    /// real rather than vacuous.
    #[test]
    fn a_credential_straddling_the_input_bound_does_not_survive_it() {
        // A Google key positioned so that exactly SURVIVING characters of it
        // land inside the budget: enough for the canary, and short of the
        // detector's 20-character threshold (`redact.rs:1285-1291`). That is the
        // live A209-C3 shape — the fragment a cut-first renderer emits is one
        // the detector no longer recognises, so it ships in clear.
        const SURVIVING: usize = 16;
        let key = "AIzaTLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV";
        let head = "{\"command\":\"";
        let assign = "export K=";
        let pad = "echo ok; ".repeat(240);
        let lead = INPUT_BUDGET - SURVIVING - head.len() - assign.len();
        let raw = format!("{head}{}{assign}{key}\"}}", &pad[..lead]);
        assert_eq!(
            raw.find(key).unwrap() + SURVIVING,
            INPUT_BUDGET,
            "the fixture must straddle the bound, not sit past it"
        );
        assert!(raw.len() > INPUT_BUDGET);

        let out = truncate(&raw, INPUT_BUDGET);
        assert!(
            !out.contains("TLCANARY") && !out.contains("AIza"),
            "a credential crossing the {INPUT_BUDGET}-character bound shipped: {out}"
        );

        // The control: the same string cut BEFORE masking leaks, which is what
        // makes the assertion above a measurement instead of a tautology.
        let cut_first = thruline_core::redact::truncate_respecting_markers(&raw, INPUT_BUDGET);
        let (masked_after, _) = thruline_core::redact::mask_recorded(&cut_first);
        assert!(
            masked_after.contains("TLCANARY"),
            "the control did not reproduce the cut-first leak, so this test \
             proves nothing about ordering: {masked_after}"
        );
    }

    /// A credential wholly past the bound is removed by the cut, and a
    /// credential wholly inside it is masked. Both arms, so the straddling case
    /// above is not carrying the whole file.
    #[test]
    fn a_credential_inside_the_input_bound_is_masked_and_one_past_it_is_gone() {
        let key = "AIzaTLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV";

        let inside = format!("{{\"command\":\"export K={key}\"}}");
        let out = truncate(&inside, INPUT_BUDGET);
        assert!(!out.contains("TLCANARY"), "canary survived inside: {out}");
        assert!(out.contains("[masked:"), "no mask was recorded: {out}");

        let past = format!("{}export K={key}\"}}", "echo ok; ".repeat(400));
        assert!(past.find(key).unwrap() > INPUT_BUDGET);
        let out = truncate(&past, INPUT_BUDGET);
        assert!(!out.contains("TLCANARY"), "canary survived past: {out}");
        assert!(
            out.ends_with('…'),
            "a cut string must say it was cut: {out}"
        );
    }
}
