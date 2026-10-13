//! `thruline reexec` — BUILD-SPEC §7, and the gate BUILD-PLAN C.7/3 put in
//! front of it.
//!
//! # `reexecuted: true` is derived, never asserted
//!
//! Nothing here writes a boolean into a receipt. A re-execution records the
//! **container image digest it ran under** on the §3.3 `Check` row;
//! `thruline receipt` *derives* `reexecuted` by asking that row whether it has
//! one, and `thruline verify` refuses a receipt whose `reexecuted: true` has no
//! `sha256:`-shaped digest behind it — and the inverse, so a digest paired with
//! `reexecuted: false` is a record disagreeing with itself.
//!
//! The distinction is the whole point. A boolean in a predicate is a claim; a
//! digest naming an image a container was actually built and run from is
//! evidence, and the receipt cannot carry the first without the second.
//!
//! ## Why the digest is not on a chained event, which would be stronger
//!
//! The first version of this appended an `exec` event to the session's
//! append-only chain. **It ate hook deliveries**, and the mechanism is the one
//! this repository has already been bitten by once: the fold assigns
//! `seq = ordinal * SPOOL_SEQ_STRIDE` and resumes at `seq <= last_seq`, so any
//! foreign writer that advances the chain tail into the next ordinal's slot
//! makes the *next* spool line hit the resume guard and be counted as **already
//! present**. A real delivery vanishes and the report calls it idempotence —
//! verbatim the defect the stride was introduced to fix.
//!
//! Sharing the seq space needs the fold's resume point to stop being the chain
//! tail, which is the property its crash-safety and idempotence tests rest on.
//! So the digest lives on the `Check` row, which is where BUILD-SPEC §8.1 puts
//! `container_image_digest` anyway. The cost, stated: `checks` is derived and
//! carries no append-only trigger, so this digest is exactly as trustworthy as
//! the ledger — **the same standing every other `Check` in the predicate
//! already has**. A re-execution therefore leaves the chain head unchanged, and
//! a test asserts it.
//!
//! # The gate refuses; it does not annotate
//!
//! If the session's capture is degraded — a `.gitattributes` clean filter, or
//! any of C.7's fourteen rows — the tree this would materialise is **not the
//! code that ran**. Re-running honest code to confirm a claim about cheating
//! code *is* the laundering, and annotating it would leave the laundering in
//! place with a footnote. So [`thruline_reexec::Gate`] refuses, and
//! `reexecuted: true` is unreachable for that session.
//!
//! # The TTY trap, handled before any comparison
//!
//! The M0.8 recorder gives its children a pipe and records `stdio: "pipe"`; so
//! does the container. When both agree, tallies are compared. When they do not,
//! the comparison says `not_comparable` rather than reporting a digest mismatch
//! that is not one.
//!
//! # Flakiness (M3.3, BUILD-SPEC §6.3), and why it is a fourth outcome
//!
//! > *"re-run failed tests up to `policy.reruns` (default 2); a test is
//! > `flaky` if outcomes differ; flaky tests never count as passing evidence."*
//!
//! A failing run is re-run, up to the policy's cap on **total runs of the same
//! command**. If the runs disagree with each other, the command is flaky, and
//! the row records the comparison outcome `flaky` — which
//! `tests_pass::fold_reexecutions` reads as a `fail` finding, so the claim
//! cannot be `supported`.
//!
//! **The recorded outcome is the LAST run**, and that choice is what makes the
//! invariant load-bearing rather than inherited. "Flaky never counts as
//! passing" used to hold by accident: a re-execution that exited non-zero is
//! vetoed as `reexecution_failed` before anything about flakiness is consulted.
//! Under a rerun policy the interesting flake is precisely *fail-then-pass* —
//! the one whose last run exits 0 — so recording the first (failing) run would
//! have kept the accident alive and left the flakiness rule reachable by
//! nothing. Recording the last run puts the whole weight on the `flaky`
//! outcome, and nothing is lost: every run is written into the comparison
//! block, in order, with its own exit code and tally.
//!
//! **Two limits, named rather than implied.**
//!
//! 1. The detector is **one-directional**. §6.3 re-runs *failed* tests, so a
//!    command that passes on its first run is never run twice and a
//!    pass-then-fail flake is invisible here. Running unconditionally would
//!    catch it and would double the cost of every re-execution; the spec says
//!    what it says, so this build follows it and says so out loud.
//!
//!    **This is a scope limit, decided, not a gap left open by accident** — and
//!    it is held by a test rather than by this paragraph.
//!    `m3_done_when_a_claimed_pytest_run_is_re_executed_in_a_clean_container`
//!    asserts a passing command produced exactly ONE run. A prose limit nothing
//!    can falsify is the same defect as a prose guarantee nothing can falsify;
//!    the assertion is what makes widening the policy a deliberate act instead
//!    of a drift. It is also listed under *asserted, not proven* in
//!    `docs/STATUS.md`, because what is proven is that the build behaves as
//!    described — not that the described behaviour catches every flake.
//!
//!    A second consequence, named here because it is the same seam: the tally
//!    half of [`Attempt::outcome`] can only DECIDE anything when two runs share
//!    an exit code, and a rerun happens only after a failure — so both runs are
//!    non-zero, and `Report::exit_code` and `tests_pass`'s `reexecution_failed`
//!    have already refused on the exit code alone. The tally therefore changes
//!    the RECORD (`deterministic`, and the stored row's `flaky`) and cannot
//!    today change a verdict. Witnessed by
//!    `two_runs_with_one_exit_code_and_two_tallies_are_flaky`.
//! 2. A **timed-out** run is not re-run. The budget it blew is the reason, and
//!    "it timed out and then passed" is not a distinction worth a second
//!    budget.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use thruline_core::spec::event::EventId;
use thruline_ledger::{CheckRow, Ledger};
use thruline_reexec::docker::{minimal_env, DockerSandbox};
use thruline_reexec::{
    Comparison, Gate, PrepareRequest, Prepared, Refusal, RunFacts, RunRequest, Sandbox,
};

pub struct Args {
    pub session: Option<String>,
    /// `claimed` re-runs exactly the checks the agent ran; `full` runs the
    /// repository's canonical test command.
    pub mode: String,
    /// `docker` is the only backend §7 makes mandatory. The cloud backend is a
    /// feature flag the spec defers.
    pub backend: String,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub timeout_secs: u64,
    pub json: bool,
}

/// What one re-executed check produced.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReexecutedCheck {
    pub original_check_id: String,
    pub check_id: String,
    pub argv: Vec<String>,
    pub runner: String,
    /// The agent's own exit code for this command. **`None` under
    /// `--mode full`**, where the agent never ran it.
    ///
    /// It used to be a bare `i64` carrying the type's minimum value as a
    /// sentinel, and the sentinel reached the JSON contract:
    /// `"original_exit_code": -9223372036854775808`. A value the process invented, serialised where a
    /// recorded one belongs, is the same defect as `CheckView::seq`'s old
    /// `u64::MAX` one layer along — so the type no longer has room for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_exit_code: Option<i64>,
    pub exit_code: i64,
    pub duration_ms: u64,
    pub timed_out: bool,
    /// `None` under `--mode full`: the agent never ran this command, so there
    /// is nothing to compare it against. **Distinct from `NotComparable`**,
    /// which means the two runs exist and could not be compared — the first is
    /// a mode with no comparison to make, the second is a comparison that
    /// failed, and only the second is a reason to withhold success.
    pub comparison: Option<Comparison>,
    pub parsed: Value,
    /// Every run of this command, in order — one entry when the first run
    /// passed, up to `policy.reexec.reruns` when it did not.
    pub runs: Vec<RunRecord>,
    /// Did every run produce the same outcome? **`false` is flaky**, and §6.3
    /// says a flaky run is never passing evidence.
    pub deterministic: bool,
}

/// One run of one command in the sandbox.
///
/// The list of these is the evidence behind `deterministic`, and it is written
/// into the stored row as well as this report: a reader asked to accept
/// "flaky" is entitled to see the runs that made it so.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunRecord {
    /// 1-based, so `run: 2` reads as "the second run" in a report a human sees.
    pub run: u32,
    pub exit_code: i64,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub parsed: Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Report {
    pub session: String,
    pub mode: String,
    pub tree_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<Prepared>,
    pub checks: Vec<ReexecutedCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<Refusal>,
    /// The exit code this report corresponds to, carried so a `--json`
    /// consumer and a `$?` consumer read the same conclusion.
    pub exit: u8,
}

impl Report {
    /// **0 ran and agreed · 2 ran and could not be compared · 3 refused,
    /// disagreed, or failed at the final tree.**
    ///
    /// The middle code is the one that matters. Without it, a re-execution
    /// whose output could not be parsed, or one run against a differently
    /// captured original, exits 0 — and a CI gate reads 0 as a pass. That is
    /// §6's `unverifiable` → `supported` collapse wearing §7's clothes, and it
    /// is the same three-way distinction `verify-receipt` already draws, in the
    /// same codes.
    fn exit_code(&self) -> u8 {
        if self.refused.is_some() {
            return 3;
        }
        if self.checks.iter().any(|c| {
            matches!(c.comparison, Some(Comparison::Disagrees { .. }))
                // A re-run that FAILED at the final tree refutes the claim it
                // was re-running, whether or not the agent's run agreed.
                || c.exit_code != 0
                // A run killed by the budget produced no result at all.
                || c.timed_out
                // §6.3: flaky is never passing evidence. Without this clause a
                // fail-then-pass flake exits 0 — the recorded outcome is the
                // passing run — and a CI gate reads 0 as a pass.
                || !c.deterministic
        }) {
            return 3;
        }
        if self
            .checks
            .iter()
            .any(|c| matches!(c.comparison, Some(Comparison::NotComparable { .. })))
        {
            return 2;
        }
        0
    }
}

/// One command the re-execution will run, and what the agent's own run of it
/// produced — `None` under `--mode full`, where there is nothing to compare
/// against.
struct Planned {
    original_check_id: String,
    runner: String,
    argv: Vec<String>,
    /// `None` under `--mode full`: the agent never ran this command.
    original_exit: Option<i64>,
    /// How the agent's own run was connected, **as its event recorded it**.
    /// `None` when nothing recorded it, which is not the same as `pipe`.
    original_stdio: Option<String>,
    original_tally: Option<(u64, u64, u64)>,
}

pub fn run(args: Args) -> Result<ExitCode> {
    if args.backend != "docker" {
        anyhow::bail!(
            "backend {:?} is not built. §7 makes the Docker backend mandatory and puts the \
             cloud backend behind a feature flag that this build does not ship; naming an \
             unbuilt backend must fail rather than quietly falling back to another one.",
            args.backend
        );
    }
    if args.mode != "claimed" && args.mode != "full" {
        anyhow::bail!("mode must be `claimed` or `full`, not {:?}", args.mode);
    }

    let db = match args.exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    if !db.is_file() {
        anyhow::bail!("no execution store at {}", db.display());
    }
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let repo = args
        .repo
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    // Read before a container is built, so a policy file with a typo fails
    // fast rather than after a minute of `docker build`.
    let policy = load_policy(&repo)?;
    let session = match args.session {
        Some(s) => s,
        None => crate::claims_cmd::last_session(&ledger)?.context("no session in the store")?,
    };
    let events = ledger
        .session_events(&session)
        .with_context(|| format!("reading session {session}"))?;
    let view = crate::claims_cmd::session_view(&session, &events, &ledger, &repo)?;

    // --- the gate ---------------------------------------------------------
    // A check whose command the record does not hold cannot be re-executed:
    // there is nothing to run. The argv is carried out of the filter rather
    // than re-read afterwards, so no unwrap stands between the test and the
    // command it runs.
    let candidates: Vec<(&thruline_verify::binding::CheckView, Vec<String>)> = view
        .checks
        .iter()
        .filter(|c| c.runner != "unknown")
        // A previous re-execution is not a claimed check. Re-running one
        // compares a container against a container and says nothing about what
        // the agent did.
        .filter(|c| c.reexecution.is_none())
        .filter_map(|c| match c.argv.as_deref() {
            Some(argv) if !argv.is_empty() => Some((c, argv.to_vec())),
            _ => None,
        })
        .collect();

    // **Tree fidelity is asked HERE, now, about the tree this is about to
    // materialise — not only about what a snapshot happened to record.**
    //
    // The session's `capture_degraded` flag has exactly one producer: the exec
    // recorder, which stamps it on a snapshot when a filter driver was in scope
    // *at an exec boundary*. So an agent that installs `.gitattributes` after
    // its last wrapped exec leaves every recorded snapshot clean, the gate says
    // nothing, and `reexec` materialises a tree that does not name the bytes on
    // disk — which is the laundering the gate exists to refuse. Measured, not
    // theorised: the identical filter refuses when installed one exec earlier
    // and is silent when installed one exec later.
    //
    // Asking git directly costs one snapshot, which is nothing beside building
    // a container, and it cannot be dodged by ordering.
    let live = thruline_core::spec::snapshot::snapshot_tree(&repo).with_context(|| {
        format!(
            "reading the content filters in scope for {} before materialising its tree. \
             Re-execution cannot proceed on a tree whose fidelity it could not check.",
            repo.display()
        )
    })?;
    let (capture_degraded, degraded_reason) = if view.capture_degraded {
        (true, view.degraded_reason.clone())
    } else if live.content_filters.degrades_capture() {
        (
            true,
            Some(format!(
                "{} — in scope now, at re-execution time. The session's own snapshots were \
                 taken before it was installed, so nothing in the record flags it",
                live.content_filters.describe()
            )),
        )
    } else {
        (false, None)
    };

    let gate = Gate {
        capture_degraded,
        degraded_reason,
        final_tree_hash: view.final_tree_hash().map(str::to_string),
        claimed_checks: if args.mode == "full" {
            // `full` supplies its own command, so an absence of recognised
            // checks is not a reason to refuse.
            1
        } else {
            candidates.len()
        },
    };
    let tree = match gate.check() {
        Ok(tree) => tree.to_string(),
        Err(refusal) => {
            let report = Report {
                session,
                mode: args.mode,
                tree_hash: view.final_tree_hash().unwrap_or_default().to_string(),
                sandbox: None,
                checks: Vec::new(),
                exit: 3,
                refused: Some(refusal.clone()),
            };
            emit(&report, args.json);
            return Ok(ExitCode::from(report.exit));
        }
    };

    // --- what to run ------------------------------------------------------
    let planned: Vec<Planned> = if args.mode == "full" {
        let argv = canonical_command(&repo).with_context(|| {
            format!(
                "no canonical test command could be detected for {}. §7 takes it from \
                     `.thruline/policy.yml`, which the policy engine (§9, M6) does not read \
                     yet — name the command in `.thruline/reexec.yml`'s `install:` \
                     environment or run `--mode claimed`.",
                repo.display()
            )
        })?;
        vec![Planned {
            runner: argv.first().cloned().unwrap_or_default(),
            argv,
            original_check_id: String::new(),
            original_exit: None,
            original_stdio: None,
            original_tally: None,
        }]
    } else {
        candidates
            .iter()
            .map(|(c, argv)| Planned {
                original_check_id: c.id.clone(),
                runner: c.runner.clone(),
                argv: argv.clone(),
                original_exit: Some(c.exit_code),
                original_stdio: c.stdio.clone(),
                original_tally: c
                    .parsed
                    .as_ref()
                    .map(|p| (p.tests_total, p.passed, p.failed)),
            })
            .collect()
    };

    // --- prepare ----------------------------------------------------------
    // ONE read of `.thruline/reexec.yml`, and it may refuse.
    //
    // This was two reads: `image::select` with `?`, then a second one written
    // `unwrap_or(None)` for the network bit. A config that would not parse
    // therefore became `None` here, and `None` becomes `network: false` — a
    // degrade in the safe direction, which is why nobody saw it. It was also
    // dead code in practice, because `select` refused on the same bytes one
    // line earlier: a guard riding a stronger veto, live the moment the veto
    // moves. Measured: with `select`'s read removed, `reexec` ran the whole
    // re-execution and exited 0 on a config it could not read.
    //
    // The repair is a read that is hoisted, not a check that is added. There is
    // now no second reader for this one to ride, so turning the `?` below back
    // into `unwrap_or(None)` reddens
    // `an_unreadable_reexec_config_refuses_instead_of_degrading_to_no_network`.
    let config = thruline_reexec::ReexecConfig::load(&repo).with_context(|| {
        format!(
            "reading {}",
            repo.join(thruline_reexec::ReexecConfig::PATH).display()
        )
    })?;
    let network = config.as_ref().and_then(|c| c.network.as_deref()) == Some("host");
    let image = thruline_reexec::image::select_with(&repo, config.as_ref())
        .context("selecting a container image")?;
    let mut sandbox = DockerSandbox::new();
    let prepared = sandbox
        .prepare(&PrepareRequest {
            repo: repo.clone(),
            tree_hash: tree.clone(),
            image,
            network,
        })
        .context("preparing a clean container at the final tree")?;

    if !prepared.materialized.complete() {
        let refusal = Refusal::SubmoduleContentAbsent {
            paths: prepared.materialized.gitlinks.keys().cloned().collect(),
        };
        let _ = sandbox.destroy();
        let report = Report {
            session,
            mode: args.mode,
            tree_hash: tree,
            sandbox: None,
            checks: Vec::new(),
            exit: 3,
            refused: Some(refusal),
        };
        emit(&report, args.json);
        return Ok(ExitCode::from(report.exit));
    }

    // --- run --------------------------------------------------------------
    // M3.3: how many times one command may be run. `reruns` is a **cap on
    // total runs**, not a count of extra ones, so the default 2 means "run it,
    // and if it failed run it once more" — which is the shape §6.3 describes.
    let reruns_max = rerun_cap(&policy);
    let mut done = Vec::new();
    let mut checks: Vec<CheckRow> = Vec::new();

    for Planned {
        original_check_id,
        runner,
        argv,
        original_exit,
        original_stdio,
        original_tally,
    } in planned
    {
        // M3.3's loop. One run, then re-runs while the previous one FAILED and
        // the cap allows another. A timed-out run is not re-run: see the
        // module docs.
        let mut attempts: Vec<Attempt> = Vec::new();
        loop {
            let attempt = run_once(&mut sandbox, &argv, Duration::from_secs(args.timeout_secs))?;
            let again = attempt.result.exit_code != 0 && !attempt.result.timed_out && {
                let n = u32::try_from(attempts.len()).unwrap_or(u32::MAX);
                n.saturating_add(1) < reruns_max
            };
            attempts.push(attempt);
            if !again {
                break;
            }
        }
        // **The last run is the recorded one.** See the module docs: recording
        // the first (failing) one would leave `reexecution_failed` carrying
        // every fail-then-pass flake and the flakiness rule reachable by
        // nothing.
        let deterministic = attempts
            .windows(2)
            .all(|w| w[0].outcome() == w[1].outcome());
        let runs: Vec<RunRecord> = attempts
            .iter()
            .enumerate()
            .map(|(i, a)| RunRecord {
                run: u32::try_from(i + 1).unwrap_or(u32::MAX),
                exit_code: a.result.exit_code,
                duration_ms: a.result.duration_ms,
                timed_out: a.result.timed_out,
                parsed: a.parsed_value.clone(),
            })
            .collect();
        let Some(last) = attempts.pop() else {
            // The loop above always pushes before it can break, so this is
            // unreachable; it is written as a refusal rather than an `expect`
            // because `crates/cli` may only `expect` on startup config.
            anyhow::bail!("no run was performed for {argv:?}");
        };
        let Attempt {
            result,
            category,
            parsed_value,
            tally,
        } = last;

        let comparison = if let Some(original_exit) = original_exit {
            Some(Comparison::of(
                &RunFacts {
                    exit_code: original_exit,
                    tally: original_tally,
                    // **Read, not asserted.** Every exec the recorder writes
                    // carries `stdio` for exactly this comparison; hardcoding
                    // `"pipe"` here made both sides equal by construction and
                    // §7's capture-shape branch unreachable. An unrecorded mode
                    // is `unknown`, which differs from anything the sandbox
                    // reports and therefore withholds the output comparison
                    // rather than assuming it away.
                    stdio: original_stdio.unwrap_or_else(|| "unknown".to_string()),
                },
                &RunFacts {
                    exit_code: result.exit_code,
                    tally,
                    stdio: result.stdio.to_string(),
                },
            ))
        } else {
            // Not `NotComparable`: there is no second run to compare with, which
            // is a property of the mode rather than a failure to compare.
            None
        };

        let check_id = reexec_check_id(&session, &prepared.image_digest, &argv, &original_check_id);
        checks.push(CheckRow {
            id: check_id.clone(),
            session_id: session.clone(),
            // There is no `exec` event: see the module docs on why a
            // re-execution does not write into the session's chain. An empty
            // id can never equal an event id, so nothing can mistake this for
            // a captured exec.
            exec_event_id: String::new(),
            category,
            runner: runner.clone(),
            argv_hash: format!(
                "sha256:{}",
                thruline_core::chain::sha256_hex(argv.join("\u{0}").as_bytes())
            ),
            cwd_hash: format!("sha256:{}", thruline_core::chain::sha256_hex(b"/w")),
            env_hash: format!(
                "sha256:{}",
                thruline_core::chain::sha256_hex(
                    minimal_env()
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join("\u{0}")
                        .as_bytes()
                )
            ),
            input_tree_hash: tree.clone(),
            exit_code: result.exit_code,
            parsed: record_comparison(
                thruline_reexec::parsed_block(
                    &parsed_value,
                    &prepared,
                    &args.mode,
                    &original_check_id,
                    &argv,
                    &result,
                ),
                comparison.as_ref(),
                &runs,
                deterministic,
            )
            .to_string(),
        });

        done.push(ReexecutedCheck {
            original_check_id,
            check_id,
            argv,
            runner,
            original_exit_code: original_exit,
            exit_code: result.exit_code,
            duration_ms: result.duration_ms,
            timed_out: result.timed_out,
            comparison,
            parsed: parsed_value,
            runs,
            deterministic,
        });
    }

    let _ = sandbox.destroy();

    for row in &checks {
        ledger
            .insert_check(row)
            .with_context(|| format!("recording check {}", row.id))?;
    }

    let report = Report {
        session,
        mode: args.mode,
        tree_hash: tree,
        sandbox: Some(prepared),
        checks: done,
        exit: 0,
        refused: None,
    };
    let report = Report {
        exit: report.exit_code(),
        ..report
    };
    emit(&report, args.json);
    Ok(ExitCode::from(report.exit))
}

/// Put §7's comparison **on the row**, beside the image digest.
///
/// The comparison used to live only in this command's own report — printed,
/// exit-coded, and then dropped. So `thruline claims`, reading the store
/// afterwards, could not tell a re-execution that *confirmed* the agent's run
/// from one that merely *ran*: both were a passing check stamped with the
/// final tree hash. Everything §6.3 now does with a re-execution — confirm,
/// contradict, or refuse to promote — needs this field to exist in the record
/// rather than in a process that has exited.
///
/// Written here rather than inside `thruline_reexec::parsed_block` because the
/// comparison is made here; a `parsed_block` that took a comparison it could
/// not compute would be a parameter the caller could lie in.
/// # `flaky` is a fourth outcome, and it outranks the other three
///
/// When the runs disagreed with each other, what this re-execution establishes
/// about the agent's run is neither agreement nor disagreement: it is that the
/// command does not produce one answer at this tree in this image, so no run of
/// it — the agent's included — is evidence that the suite passes. That is a
/// different and stronger statement than `not_comparable`, which says the two
/// runs could not be lined up. `tests_pass` reads `flaky` as a `fail`.
///
/// The comparison the last run *did* produce is kept beside it as `final_run`,
/// and every run is kept as `runs`, so nothing the process saw is dropped on
/// the way into the store.
///
/// [`thruline_reexec::Comparison`] has no `Flaky` variant, so this outcome is
/// written as JSON here rather than serialised from the enum. That is a seam:
/// the enum should grow the variant, and until it does this doc comment is the
/// only thing keeping the two spellings of the outcome set together.
fn record_comparison(
    mut parsed: Value,
    comparison: Option<&Comparison>,
    runs: &[RunRecord],
    deterministic: bool,
) -> Value {
    let Some(block) = parsed.get_mut("reexec").and_then(Value::as_object_mut) else {
        return parsed;
    };
    let final_run = comparison.and_then(|c| serde_json::to_value(c).ok());
    if !deterministic {
        let mut flaky = json!({
            "outcome": "flaky",
            "detail": format!(
                "the same command was run {} times at the same tree in the same image and \
                 produced different outcomes ({}); §6.3: a flaky run is never passing \
                 evidence",
                runs.len(),
                runs.iter()
                    .map(|r| format!("run {} exited {}", r.run, r.exit_code))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "runs": runs,
        });
        if let (Some(map), Some(final_run)) = (flaky.as_object_mut(), final_run) {
            map.insert("final_run".into(), final_run);
        }
        block.insert("comparison".into(), flaky);
        return parsed;
    }
    // Absent, not `null`: `--mode full` made no comparison, and "no comparison
    // was made" must not read as "a comparison came back empty".
    if let Some(mut value) = final_run {
        // More than one run and they all agreed: say so, rather than leaving a
        // reader to assume a single run.
        if runs.len() > 1 {
            if let Some(map) = value.as_object_mut() {
                map.insert("runs".into(), json!(runs));
            }
        }
        block.insert("comparison".into(), value);
    }
    parsed
}

/// One run of one command, and what it parsed to.
struct Attempt {
    result: thruline_reexec::ExecResult,
    category: String,
    parsed_value: Value,
    tally: Option<(u64, u64, u64)>,
}

impl Attempt {
    /// What "the same outcome" means for M3.3.
    ///
    /// The exit code and the tally together. Two runs that exit 0 and report
    /// different numbers of tests are as flaky as two that exit differently —
    /// §6.3's word is *outcomes*, not *exit codes* — and a run whose output
    /// did not parse has a different outcome from one that did, because the
    /// second knows something the first does not.
    fn outcome(&self) -> (i64, Option<(u64, u64, u64)>) {
        (self.result.exit_code, self.tally)
    }
}

fn run_once(sandbox: &mut impl Sandbox, argv: &[String], timeout: Duration) -> Result<Attempt> {
    let result = sandbox
        .run(&RunRequest {
            argv: argv.to_vec(),
            cwd: String::new(),
            env: minimal_env(),
            timeout,
        })
        .with_context(|| format!("re-executing {argv:?}"))?;
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let outcome = thruline_verify::check::check_from_exec(argv, result.exit_code, &output);
    let (category, parsed_value, tally) = match outcome.draft() {
        Some(draft) => {
            let (value, tally) = parsed_of(&draft.outcome);
            (draft.category.to_string(), value, tally)
        }
        None => (
            "custom".to_string(),
            json!({
                "unverifiable": "no runner was recognised in the re-executed command"
            }),
            None,
        ),
    };
    Ok(Attempt {
        result,
        category,
        parsed_value,
        tally,
    })
}

/// M3.3's cap on how many times one command is run, from `.thruline/policy.yml`.
///
/// **The policy may raise it and may not lower it.** `reruns` is the repository's
/// own setting, and the repository is the party whose claims are being checked;
/// a `reruns: 1` in a file the agent can write would switch off the flakiness
/// detector from inside the thing being measured. So the shipped default is a
/// floor, and a policy asking for less is reported rather than silently
/// applied — the alternative, obeying it, is the one shape of setting this
/// product cannot offer.
fn rerun_cap(policy: &thruline_policy::Policy) -> u32 {
    let asked = policy.reexec.reruns;
    let floor = thruline_policy::Policy::default().reexec.reruns;
    if asked < floor {
        eprintln!(
            "`.thruline/policy.yml` asks for reexec.reruns = {asked}; {floor} is the floor \
             and is being used. A repository cannot lower the number of times ThruLine \
             re-runs its own failing test command."
        );
    }
    asked.max(floor)
}

/// The repository's policy, or the shipped default when it has none.
///
/// A policy file that does not parse is an error rather than a fallback: §9's
/// settings change what evidence is demanded, and quietly running under
/// defaults because a file had a typo is the same class of silence as a
/// detector that cannot look.
fn load_policy(repo: &Path) -> Result<thruline_policy::Policy> {
    let path = repo.join(".thruline").join("policy.yml");
    match std::fs::read_to_string(&path) {
        Ok(text) => thruline_policy::Policy::parse(&text)
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
            .context("the policy file was not understood"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(thruline_policy::Policy::default())
        }
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// A stable id for a re-executed check.
///
/// Derived rather than random, so re-running `reexec` against the same tree in
/// the same image is a no-op on the store (`insert_check` is
/// `ON CONFLICT DO NOTHING`) rather than a second row saying the same thing.
fn reexec_check_id(session: &str, image_digest: &str, argv: &[String], of_check: &str) -> String {
    // `of_check` is in the material because an agent may run the SAME command
    // twice in one session; without it the second re-execution would derive the
    // first's id and `ON CONFLICT DO NOTHING` would silently keep one record
    // for two runs.
    let derived = EventId::derive(
        format!(
            "reexec|{session}|{image_digest}|{of_check}|{}",
            argv.join("\u{0}")
        )
        .as_bytes(),
    );
    format!("chk_{}", derived.as_str().trim_start_matches("evt_"))
}

/// The §3.3 `parsed` block, and the tally a comparison may use.
fn parsed_of(outcome: &thruline_verify::RunnerOutcome) -> (Value, Option<(u64, u64, u64)>) {
    match outcome {
        thruline_verify::RunnerOutcome::Parsed(p) => (
            json!({
                "tests_total": p.tests_total,
                "passed": p.passed,
                "failed": p.failed,
                "skipped": p.skipped,
                "xfail": p.xfail,
            }),
            Some((p.tests_total, p.passed, p.failed)),
        ),
        // Never `{}`: a defaulted parsed block reads as a clean run.
        other => (json!({ "unverifiable": format!("{other:?}") }), None),
    }
}

/// `--mode full`'s command, detected from the repository.
///
/// §7 says this comes from `.thruline/policy.yml` or is detected. The policy
/// engine is §9/M6, so only detection exists, and it is named as such rather
/// than pretending the policy path is wired.
fn canonical_command(repo: &Path) -> Option<Vec<String>> {
    use thruline_reexec::image::{detect_ecosystem, Ecosystem};
    match detect_ecosystem(repo)? {
        Ecosystem::Python => Some(vec!["pytest".into(), "-q".into()]),
        Ecosystem::Rust => Some(vec!["cargo".into(), "test".into()]),
        Ecosystem::Node => Some(vec!["npm".into(), "test".into()]),
        Ecosystem::Go => Some(vec!["go".into(), "test".into(), "./...".into()]),
        Ecosystem::Jvm => None,
    }
}

fn emit(report: &Report, json: bool) {
    if json {
        match serde_json::to_value(report)
            .map_err(anyhow::Error::from)
            .and_then(|v| crate::json_out::print(&crate::json_out::REEXEC, v))
        {
            Ok(()) => {}
            Err(e) => eprintln!("could not render the report: {e}"),
        }
        return;
    }
    if let Some(refusal) = &report.refused {
        println!("REFUSED — {}", refusal.message());
        return;
    }
    if let Some(s) = &report.sandbox {
        println!(
            "re-executed at tree {} in {} ({}), network {}",
            &report.tree_hash[..report.tree_hash.len().min(12)],
            s.image,
            s.image_digest,
            if s.network { "host" } else { "none" }
        );
    }
    for c in &report.checks {
        if !c.deterministic {
            println!(
                "FLAKY {} → {} runs, exit codes {} — §6.3: a flaky run is never passing \
                 evidence",
                c.argv.join(" "),
                c.runs.len(),
                c.runs
                    .iter()
                    .map(|r| r.exit_code.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            continue;
        }
        let (mark, detail) = match &c.comparison {
            Some(Comparison::Agrees { detail }) => ("ok  ", detail.as_str()),
            Some(Comparison::Disagrees { detail }) => ("FAIL", detail.as_str()),
            Some(Comparison::NotComparable { detail }) => ("n/c ", detail.as_str()),
            None => (
                "ran ",
                "the agent did not run this, so there is nothing to compare it against",
            ),
        };
        println!(
            "{mark} {} → exit {} — {detail}",
            c.argv.join(" "),
            c.exit_code
        );
    }
    match report.exit {
        3 => println!(
            "\nThe re-execution refutes the record: it disagreed with the agent's run, \
             failed at the final tree, or did not produce the same answer twice. §7: \
             mismatches are `unsupported`, with both records attached; §6.3: a flaky \
             run is never passing evidence."
        ),
        2 => println!(
            "\nThe re-execution ran and could not be compared. That is not agreement, and \
             it is not a failure either — the same distinction `verify-receipt` draws."
        ),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_with_reruns(n: u32) -> String {
        format!("version: 1\nreexec:\n  mode: claimed\n  reruns: {n}\n  image: auto\n")
    }

    #[test]
    fn the_policy_may_raise_the_rerun_cap() {
        let policy = thruline_policy::Policy::parse(&policy_with_reruns(5)).expect("parse");
        assert_eq!(rerun_cap(&policy), 5);
    }

    #[test]
    fn the_policy_may_not_lower_the_rerun_cap() {
        // The repository is the party being checked. A `reruns: 1` in a file
        // the agent can write would switch the flakiness detector off from
        // inside the thing being measured, so the shipped default is a floor.
        let floor = thruline_policy::Policy::default().reexec.reruns;
        for asked in 0..floor {
            let policy = thruline_policy::Policy::parse(&policy_with_reruns(asked)).expect("parse");
            assert_eq!(
                rerun_cap(&policy),
                floor,
                "a policy asking for reexec.reruns = {asked} lowered the cap below the \
                 shipped floor of {floor}"
            );
        }
    }

    #[test]
    fn a_repository_with_no_policy_gets_the_shipped_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let policy = load_policy(dir.path()).expect("no policy is not an error");
        assert_eq!(policy, thruline_policy::Policy::default());
    }

    #[test]
    fn a_policy_that_does_not_parse_is_an_error_and_not_a_default() {
        // Rule 12. Running under defaults because a file had a typo is the
        // same silence as a detector that cannot look.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".thruline")).expect("mkdir");
        std::fs::write(
            dir.path().join(".thruline/policy.yml"),
            "version: 1\nreexec:\n  reruns: not-a-number\n",
        )
        .expect("write");
        assert!(
            load_policy(dir.path()).is_err(),
            "a policy file that does not parse was silently replaced by the default"
        );
    }
}
