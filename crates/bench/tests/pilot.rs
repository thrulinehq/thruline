//! The pilot path: prove the pipeline against a live agent without spending the
//! holdout.
//!
//! Nothing here runs an agent. Every assertion is about a **refusal** — the
//! pilot's job is to make the full run's decision informed, and each of these
//! is a way that decision could be made on a number meaning something other
//! than it appears to.

use std::collections::BTreeMap;

use thruline_bench::pilot::{
    check_from_tune, check_new_tasks, plan, require_credential, scan_artifacts, tune_split,
    CaptureCoverage, CostReport, Harness, PilotError, PilotTask, TaskCost, COVERAGE_GATE,
};
use thruline_bench::{Preconditions, Protocol};

fn base(n: usize) -> Protocol {
    let mut p = Protocol {
        harness: "terminal-bench".into(),
        harness_version: "0.2.16".into(),
        model: "some-model@2026-09".into(),
        tasks: (0..n).map(|i| format!("task-{i:03}")).collect(),
        holdout: Vec::new(),
        parameters: BTreeMap::from([("seed".to_string(), "7".to_string())]),
        detectors: BTreeMap::from([("narrowed_run".to_string(), "enabled".to_string())]),
        preconditions: Preconditions::default(),
    };
    p.derive_holdout(0.3).expect("derive");
    p
}

fn task(id: &str, hooks: u64, shims: u64, checks: u64) -> PilotTask {
    PilotTask {
        task_id: id.into(),
        hook_events: hooks,
        shim_exec_events: shims,
        checks_bound_to_a_tree: checks,
        tree_before_first_action: Some("a".repeat(40)),
        cost: Some(TaskCost {
            tokens_in: 1_000,
            tokens_out: 500,
            wall_ms: 60_000,
        }),
        terminal_state: "completed".into(),
    }
}

// ---------------------------------------------------------------------------
// The holdout is not touched.
// ---------------------------------------------------------------------------

/// **Tune split only.** Stricter than "disjoint from the holdout": the pilot
/// draws from tune, so the holdout stays entirely unspent until the full run.
#[test]
fn a_pilot_draws_only_from_the_tune_split() {
    let b = base(120);
    let pilot = plan(&b, 20, Harness::ClaudeCode, &[]).expect("plan");
    assert_eq!(pilot.tasks.len(), 20);

    let tune = tune_split(&b.tasks, &b.holdout);
    for t in &pilot.tasks {
        assert!(tune.contains(t), "`{t}` is not in the tune split");
        assert!(
            !b.holdout.contains(t),
            "`{t}` is in the real holdout; the pilot would contaminate it"
        );
    }
    // The pilot reports over everything it ran.
    assert_eq!(pilot.holdout, pilot.tasks);
}

/// And a pilot protocol assembled by hand gets the same refusal — the rule is
/// checked on the protocol, not only produced by the planner.
#[test]
fn a_hand_built_pilot_that_reaches_into_the_holdout_is_refused() {
    let b = base(120);
    let mut evil = plan(&b, 5, Harness::ClaudeCode, &[]).expect("plan");
    let stolen = b.holdout[0].clone();
    evil.tasks.push(stolen.clone());

    match check_from_tune(&evil, &b) {
        Err(PilotError::NotFromTuneSplit { ids }) => assert_eq!(ids, [stolen]),
        other => panic!("{other:?}"),
    }
    let msg = check_from_tune(&evil, &b).unwrap_err().to_string();
    assert!(msg.contains("tune"), "{msg}");
    assert!(msg.contains("unspent"), "{msg}");

    // The honest one passes, so the check is about the stolen id.
    check_from_tune(&plan(&b, 5, Harness::ClaudeCode, &[]).expect("plan"), &b)
        .expect("a tune-only pilot is accepted");
}

/// **A re-pilot uses NEW tasks.** A second pass over tasks whose failures are
/// already known is a tuning run wearing a pilot's name.
#[test]
fn a_repilot_may_not_reuse_tasks_a_previous_pilot_ran() {
    let b = base(120);
    let first = plan(&b, 20, Harness::ClaudeCode, &[]).expect("plan");
    let second = plan(&b, 20, Harness::ClaudeCode, &first.tasks).expect("plan");
    for t in &second.tasks {
        assert!(!first.tasks.contains(t), "`{t}` was piloted already");
    }
    check_new_tasks(&second, &first.tasks).expect("disjoint");

    match check_new_tasks(&first, &first.tasks) {
        Err(PilotError::ReusesPilotedTasks { ids }) => assert_eq!(ids.len(), 20),
        other => panic!("{other:?}"),
    }
}

/// The real protocol is never committed by planning a pilot: the pilot has its
/// own commitment, and the two hashes differ.
#[test]
fn the_pilot_commits_its_own_protocol_and_leaves_the_real_one_alone() {
    let b = base(120);
    let pilot = plan(&b, 20, Harness::ClaudeCode, &[]).expect("plan");
    let pc = pilot.commit().expect("the pilot commits");
    assert_ne!(pc.hash(), b.hash(), "the pilot reused the real commitment");
    assert_eq!(pc.tasks().len(), 20);

    // The commitment says it is a pilot, and of what.
    assert_eq!(
        pilot.parameters.get("pilot").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        pilot.parameters.get("pilot_of").map(String::as_str),
        Some(b.hash_without_holdout()).as_deref()
    );
    assert_eq!(
        pilot.parameters.get("pilot_harness").map(String::as_str),
        Some("claude-code")
    );
    // Everything else runs identically.
    assert_eq!(pilot.preconditions, b.preconditions);
    assert_eq!(pilot.detectors, b.detectors);
}

/// Deterministic: the same base protocol plans the same pilot, so nobody
/// chooses the sample.
#[test]
fn the_pilot_sample_is_derived_not_chosen() {
    let b = base(120);
    let a = plan(&b, 20, Harness::ClaudeCode, &[]).expect("plan");
    let c = plan(&b, 20, Harness::ClaudeCode, &[]).expect("plan");
    assert_eq!(a.tasks, c.tasks);

    // And a change to anything committed moves it, as it moves the holdout.
    let mut tuned = base(120);
    tuned
        .detectors
        .insert("narrowed_run".into(), "disabled".into());
    tuned.derive_holdout(0.3).expect("derive");
    assert_ne!(
        a.tasks,
        plan(&tuned, 20, Harness::ClaudeCode, &[])
            .expect("plan")
            .tasks
    );
}

// ---------------------------------------------------------------------------
// The harness, which is the part that is easy to get wrong.
// ---------------------------------------------------------------------------

/// **Hook events only exist under the real harnesses.** A pilot through
/// Terminus would exercise the shim and say nothing about the hook adapter,
/// while producing a hook-less corpus whose coverage number looks like a
/// property of the corpus.
#[test]
fn terminus_is_refused_as_a_pilot_harness() {
    let b = base(120);
    match plan(&b, 20, Harness::Terminus, &[]) {
        Err(PilotError::HarnessWithoutHooks { harness }) => assert_eq!(harness, "terminus"),
        other => panic!("{other:?}"),
    }
    let msg = plan(&b, 20, Harness::Terminus, &[])
        .unwrap_err()
        .to_string();
    assert!(msg.contains("claude-code"), "{msg}");
    assert!(msg.contains("litellm"), "{msg}");

    // **Codex: this assertion has now been true in both directions, and the
    // history is the point.** It first read "codex fires our hooks" as a bare
    // literal while `hook-install --harness codex` answered "only the
    // claude-code harness is implemented"; a container disproved it. It then
    // read `false` until the adapter was built. It is `true` again now, and it
    // is no longer a literal at all: `provides_hook_events` reads
    // `capture::harness::ALL`, the same table that decides what the installer
    // writes. Delete the Codex pack and this line goes back to `false` on its
    // own rather than by anyone remembering.
    assert!(Harness::ClaudeCode.provides_hook_events());
    assert!(
        Harness::Codex.provides_hook_events(),
        "the codex hook adapter exists; the pilot must be able to use it"
    );
    assert!(!Harness::Terminus.provides_hook_events());
    assert_eq!(Harness::Codex.hook_gap(), None);

    // And it is derived, not restated: the pilot's answer and the installer's
    // table are the same fact. A test that only read `provides_hook_events`
    // would pass just as well against a re-hardcoded literal.
    assert_eq!(
        Harness::Codex.provides_hook_events(),
        thruline_capture::harness::by_name("codex").is_some_and(|h| h.installable),
        "the pilot's answer drifted from the installer's table"
    );

    // Terminus stays refused, and for its own reason — a property of the
    // runner, not of our table. litellm fires no hooks at all.
    assert!(thruline_capture::harness::by_name("terminus").is_none());
    assert!(plan(&b, 20, Harness::Codex, &[]).is_ok());

    // Both adapters exist in Terminal-Bench.
    assert_eq!(Harness::ClaudeCode.terminal_bench_agent(), "claude-code");
    assert_eq!(Harness::Codex.terminal_bench_agent(), "codex");
    assert_eq!(Harness::ClaudeCode.hook_gap(), None);
}

// ---------------------------------------------------------------------------
// The three coverage measures, and the gate.
// ---------------------------------------------------------------------------

/// They are reported **separately** because they fail for different reasons:
/// hooks are the harness's own account, shim execs are the runner observed
/// independently, and a bound check is the only thing §6 can reason over.
#[test]
fn the_three_coverage_measures_are_independent() {
    let tasks = vec![
        task("a", 10, 3, 1),
        task("b", 10, 3, 0), // no bound check
        task("c", 0, 3, 1),  // no hooks
        task("d", 10, 0, 1), // no shim exec
    ];
    let c = CaptureCoverage::of(&tasks);
    assert_eq!(c.with_hook_events, 3);
    assert_eq!(c.with_shim_exec_events, 3);
    assert_eq!(c.with_bound_check, 3);
    assert!((c.hook_fraction() - 0.75).abs() < 1e-9);
}

/// **The gate names which measure failed**, because the fix differs.
#[test]
fn coverage_below_the_gate_is_a_refusal_that_names_the_measure() {
    // Hooks fine, shims fine, bound checks near zero — the failure the
    // 0-of-241 measurement predicted.
    let mut tasks: Vec<PilotTask> = (0..20).map(|i| task(&format!("t{i}"), 5, 5, 0)).collect();
    tasks[0].checks_bound_to_a_tree = 1;
    let c = CaptureCoverage::of(&tasks);
    let err = c.gate().expect_err("must refuse");
    let msg = err.to_string();
    assert!(msg.contains("checks bound to a git tree"), "{msg}");
    assert!(msg.contains("git init"), "{msg}");
    assert!(msg.contains("NEW tune tasks"), "{msg}");
    assert!(
        !msg.contains("hook events"),
        "a measure that cleared the gate was reported as failing: {msg}"
    );

    // All three clearing the gate passes.
    let good: Vec<PilotTask> = (0..20).map(|i| task(&format!("t{i}"), 5, 5, 2)).collect();
    CaptureCoverage::of(&good).gate().expect("clears the gate");

    // Exactly at the gate passes; one task short does not.
    let mut edge: Vec<PilotTask> = (0..20).map(|i| task(&format!("t{i}"), 5, 5, 2)).collect();
    edge[0].checks_bound_to_a_tree = 0;
    edge[1].checks_bound_to_a_tree = 0;
    assert!(
        CaptureCoverage::of(&edge).gate().is_ok(),
        "18/20 is the gate"
    );
    edge[2].checks_bound_to_a_tree = 0;
    assert!(
        CaptureCoverage::of(&edge).gate().is_err(),
        "17/20 is below it"
    );
    assert!((COVERAGE_GATE - 0.90).abs() < 1e-9);
}

/// An empty pilot cannot pass by vacuity.
#[test]
fn a_pilot_that_ran_nothing_does_not_clear_the_gate() {
    assert!(CaptureCoverage::of(&[]).gate().is_err());
}

/// The precondition, measured where it meets a live agent.
#[test]
fn the_tree_before_first_action_is_counted_per_task() {
    let mut tasks: Vec<PilotTask> = (0..4).map(|i| task(&format!("t{i}"), 1, 1, 1)).collect();
    tasks[0].tree_before_first_action = None;
    let c = CaptureCoverage::of(&tasks);
    assert_eq!(c.with_tree_before_first_action, 3);
    assert!((c.tree_fraction() - 0.75).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Cost: measured, and projected only from measurements.
// ---------------------------------------------------------------------------

#[test]
fn the_projection_comes_from_measured_tasks_and_says_how_many() {
    let tasks: Vec<PilotTask> = (0..20).map(|i| task(&format!("t{i}"), 1, 1, 1)).collect();
    let c = CostReport::of(&tasks);
    assert_eq!(c.tasks_with_cost, 20);
    assert!((c.cost_coverage() - 1.0).abs() < 1e-9);
    let per = c.per_task().expect("measured");
    assert_eq!(per.tokens_in, 1_000);
    let full = c.projected_for(241).expect("projection");
    assert_eq!(full.tokens_in, 241_000);
    assert_eq!(full.tokens_out, 120_500);

    // **The mean is over tasks that reported a cost**, not over all tasks:
    // dividing by a denominator including unmeasured tasks understates the
    // figure the operator is about to buy with.
    let mut partial = tasks.clone();
    for t in partial.iter_mut().skip(10) {
        t.cost = None;
    }
    let c = CostReport::of(&partial);
    assert_eq!(c.tasks_with_cost, 10);
    assert_eq!(c.per_task().expect("measured").tokens_in, 1_000);
    assert!((c.cost_coverage() - 0.5).abs() < 1e-9);
}

/// **No projection from zero samples.** A projection with nothing behind it is
/// an estimate wearing a measurement's name, and the operator is being asked to
/// approve a spend on it.
#[test]
fn a_projection_with_nothing_measured_is_none_rather_than_zero() {
    let mut tasks: Vec<PilotTask> = (0..5).map(|i| task(&format!("t{i}"), 1, 1, 1)).collect();
    for t in tasks.iter_mut() {
        t.cost = None;
    }
    let c = CostReport::of(&tasks);
    assert_eq!(c.per_task(), None);
    assert_eq!(c.projected_for(241), None);
    assert_eq!(c.cost_coverage(), 0.0);
}

// ---------------------------------------------------------------------------
// The credential never enters anything.
// ---------------------------------------------------------------------------

/// Presence is checked; the value is never read.
#[test]
fn a_missing_credential_fails_by_name_and_never_falls_back() {
    // A variable this process definitely does not have.
    let err =
        require_credential("THRULINE_PILOT_KEY_THAT_IS_NOT_SET").expect_err("absent must fail");
    let msg = err.to_string();
    assert!(msg.contains("THRULINE_PILOT_KEY_THAT_IS_NOT_SET"), "{msg}");
    assert!(msg.contains("unauthenticated"), "{msg}");
    assert!(msg.contains("coverage finding"), "{msg}");

    // Present passes — and `require_credential` returns `()`, so there is no
    // value for a caller to mishandle. `PATH` stands in for a set variable
    // because the workspace forbids `unsafe`, and `set_var` is unsafe: the
    // invariant that keeps this crate honest also shapes its tests.
    require_credential("PATH").expect("PATH is set in any shell this runs in");
}

/// **The source never binds the value.** The signature returns `()`, and the
/// body must not call anything that yields the string.
#[test]
fn the_credential_check_cannot_hold_the_value() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pilot.rs"),
    )
    .expect("pilot.rs");
    let at = src.find("pub fn require_credential").expect("fn");
    let body = &src[at..at + 400];
    assert!(body.contains("var_os"), "{body}");
    assert!(
        !body.contains("env::var(") && !body.contains("to_string_lossy"),
        "require_credential reads the credential's value: {body}"
    );
    // And nothing in the module logs an environment value.
    for leak in ["ANTHROPIC_API_KEY\"", "env::var(\"ANTHROPIC"] {
        assert!(
            !src.contains(leak),
            "pilot.rs names a key variable's value: {leak}"
        );
    }
}

/// **The artifact scan, wired to §4.6's own predicate.**
///
/// Terminal-Bench's `claude_code` adapter injects `ANTHROPIC_API_KEY` into the
/// container environment, so the key is genuinely present where artifacts are
/// produced. This is not hypothetical, and a key in a published trajectory
/// would be the worst possible failure of a corpus whose point is publication.
#[test]
fn a_credential_in_a_captured_artifact_is_found_by_the_redaction_layer() {
    let detector = thruline_core::redact::resembles_known_credential;

    // A real-shaped key, never a real one.
    let planted = format!("sk-ant-api03-{}", "TLCANARYaaZZ11bbYY22ccXX33ddWW44e");
    let artifacts = vec![
        (
            "trajectory/task-001.json".to_string(),
            "ordinary output".to_string(),
        ),
        (
            "trajectory/task-002.json".to_string(),
            format!("env: ANTHROPIC_API_KEY={planted}"),
        ),
    ];
    let found = scan_artifacts(&artifacts, detector);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].artifact, "trajectory/task-002.json");

    // Positive control on the detector itself: it is the real one, and it does
    // fire on the shape.
    assert!(
        detector(&planted),
        "the §4.6 predicate did not recognise the shape"
    );
    assert!(!detector("ordinary output"));

    // Clean artifacts produce no sighting, so the scan is not simply flagging
    // everything.
    let clean: Vec<(String, String)> = (0..5)
        .map(|i| (format!("t{i}"), "nothing to see".to_string()))
        .collect();
    assert!(scan_artifacts(&clean, detector).is_empty());
}

// ---------------------------------------------------------------------------
// The budget guard. $20 of credit exists; $3.00 of it is authorised for this
// pilot; the other $17.00 is RESERVED and needs a separate authorisation. The
// enforced run cap is the authorised figure, not the credit.
// ---------------------------------------------------------------------------

use thruline_bench::pilot::{
    price_for, Budget, BudgetStop, Ledger, Price, Usage, UsageSource, CLAUDE_HAIKU_4_5,
    GPT_5_6_LUNA, PRICES, USD,
};

fn usage(inp: u64, out: u64) -> Usage {
    Usage::from_claude_code(inp, out, 0, 0, 0, UsageSource::Reconciled)
}

/// A ledger on a real path, because there is no longer any other kind.
///
/// `Ledger::default()` used to be one, and `ledger.spent = 0` was one
/// statement. That is `RunManifest::first()` again — the easy constructor IS the
/// bypass — so the type now has exactly one door and the tests walk through it.
fn fresh_ledger(dir: &tempfile::TempDir) -> Ledger {
    Ledger::open(&dir.path().join("spend.tsv")).expect("fresh ledger")
}

/// The pinned model and its price, as verified against live pricing. If this
/// changes, the cost report changes with it and someone has to say so.
#[test]
fn the_pinned_model_is_the_cheapest_codex_cli_supports() {
    assert_eq!(GPT_5_6_LUNA.model, "gpt-5.6-luna");
    assert_eq!(GPT_5_6_LUNA.input_per_mtok, 200_000); // $0.20 / 1M
    assert_eq!(GPT_5_6_LUNA.output_per_mtok, 1_200_000); // $1.20 / 1M

    // A million in, a million out, at the pinned price.
    assert_eq!(usage(1_000_000, 0).cost(GPT_5_6_LUNA), 200_000);
    assert_eq!(usage(0, 1_000_000).cost(GPT_5_6_LUNA), 1_200_000);
}

/// **Rounded up, never down.** A guard that rounds a spend down can cross the
/// ceiling while reporting that it did not.
#[test]
fn cost_rounds_up() {
    // One token of output at $1.20/1M is 1.2 micro-dollars → 2, not 1.
    assert_eq!(usage(0, 1).cost(GPT_5_6_LUNA), 2);
    assert_eq!(usage(1, 0).cost(GPT_5_6_LUNA), 1);
    assert_eq!(usage(0, 0).cost(GPT_5_6_LUNA), 0);
}

/// A looping task is halted by the per-task cap, and the halt says what it cost
/// and what had been spent before it.
#[test]
fn a_looping_task_halts_the_run_between_tasks() {
    let budget = Budget::pilot_authorisation();
    let dir = tempfile::tempdir().expect("tmp");
    let mut ledger = fresh_ledger(&dir);

    // Nine ordinary tasks.
    for i in 0..9 {
        ledger
            .charge(
                &format!("t{i}"),
                usage(200_000, 40_000),
                GPT_5_6_LUNA,
                budget,
            )
            .expect("ordinary tasks are cheap");
    }
    // 9 x (200k in @ $0.20/M + 40k out @ $1.20/M) = 9 x $0.088 = $0.792.
    // Asserted exactly rather than against a guessed bound — the first version
    // of this line guessed $0.25 and was simply wrong about the arithmetic.
    let before = ledger.spent();
    assert_eq!(before, 792_000, "nine ordinary tasks");
    assert!(before < budget.run_total);

    // The tenth loops: 5M output tokens is $6.00, over the $0.50 per-task cap.
    match ledger.charge(
        "t9-loops",
        usage(1_000_000, 5_000_000),
        GPT_5_6_LUNA,
        budget,
    ) {
        Err(BudgetStop::PerTask {
            task_id,
            cap,
            spent_before,
            ..
        }) => {
            assert_eq!(task_id, "t9-loops");
            assert_eq!(cap, USD / 2);
            assert_eq!(spent_before, before);
        }
        other => panic!("a looping task was not halted: {other:?}"),
    }
    // And it was NOT charged: the run's spend is what it was.
    assert_eq!(ledger.spent(), before);
}

/// The run-total cap halts even when every individual task is under its cap.
#[test]
fn many_ordinary_tasks_still_halt_at_the_run_total() {
    let budget = Budget::pilot_authorisation();
    let dir = tempfile::tempdir().expect("tmp");
    let mut ledger = fresh_ledger(&dir);
    // $0.40 each: under the per-task cap, over the $3.00 run cap after eight.
    let each = usage(0, 333_334);
    assert!(each.cost(GPT_5_6_LUNA) < budget.per_task);

    let mut halted_after = None;
    for i in 0..20 {
        if let Err(BudgetStop::RunTotal {
            after_tasks,
            spent,
            would_be,
            cap,
        }) = ledger.charge(&format!("t{i}"), each, GPT_5_6_LUNA, budget)
        {
            // **The cap is the cap.** This used to assert `spent > cap` — which
            // was true, and was the bug: `charge` added first and compared
            // second, so a documented "$3.00 run total" permitted $3.00 plus one
            // whole per-task cap, a $3.50 tripwire. The refused charge is now
            // not applied, so what is spent is still under the cap and it is
            // `would_be` that crosses it.
            assert!(spent <= cap, "spent {spent} crossed the cap {cap}");
            assert!(would_be > cap, "the halt did not describe a crossing");
            halted_after = Some(after_tasks);
            break;
        }
    }
    assert_eq!(halted_after, Some(7), "the run cap did not halt the run");
    assert!(
        ledger.spent() <= budget.run_total,
        "spend {} crossed the $3.00 cap",
        ledger.spent()
    );
    // Inside the authorisation, because the cap IS the authorisation.
    assert!(ledger.spent() < budget.authorised);
    // A re-pilot would fit in the credit that is left — which is a fact about
    // the money, not a licence to spend it. That $17 is reserved.
    assert!(
        budget.credit_remaining(ledger.spent()) > 4 * USD,
        "no credit left for a re-pilot even if one were authorised"
    );
}

/// Ten tasks at a realistic size fit comfortably, which is why $3.00 is a cap
/// and not a target.
#[test]
fn the_expected_ten_task_pilot_is_far_inside_the_caps() {
    let budget = Budget::pilot_authorisation();
    let dir = tempfile::tempdir().expect("tmp");
    let mut ledger = fresh_ledger(&dir);
    for i in 0..10 {
        ledger
            .charge(
                &format!("t{i}"),
                usage(300_000, 60_000),
                GPT_5_6_LUNA,
                budget,
            )
            .expect("inside the caps");
    }
    // 10 × ($0.06 + $0.072) = $1.32
    assert!(ledger.spent() < budget.run_total, "{}", ledger.spent());
    assert_eq!(ledger.tasks_charged(), 10);
    let per = ledger.per_task().expect("charged");
    assert_eq!(ledger.projected_for(241), Some(per * 241));
    // The remaining balance, for the re-pilot decision. Against the CREDIT:
    // the authorisation for THIS run is $3.00 and $1.32 of it is gone, so
    // measuring re-pilot room against `authorised_remaining` would answer a
    // different question and answer it wrong.
    assert!(budget.credit_remaining(ledger.spent()) > 6 * USD);
    assert_eq!(budget.authorised_remaining(ledger.spent()), 1_680_000);
}

/// **An understated total is worse than an unknown one**, because it looks like
/// a measurement. `turn.completed.usage` omits reasoning tokens, which bill as
/// output, so a figure from it alone is a floor and the report must say so.
#[test]
fn usage_that_may_understate_is_flagged_on_the_ledger() {
    let budget = Budget::pilot_authorisation();
    let dir = tempfile::tempdir().expect("tmp");
    let mut ledger = fresh_ledger(&dir);
    ledger
        .charge(
            "t0",
            Usage::from_claude_code(1_000, 100, 0, 0, 0, UsageSource::TurnCompleted),
            GPT_5_6_LUNA,
            budget,
        )
        .expect("charged");
    assert!(
        ledger.any_may_understate(),
        "a figure that may understate the bill was recorded as if it could not"
    );
    assert!(UsageSource::TurnCompleted.may_understate());
    assert!(!UsageSource::RolloutFile.may_understate());
    assert!(!UsageSource::Reconciled.may_understate());

    let cdir = tempfile::tempdir().expect("tmp");
    let mut clean = fresh_ledger(&cdir);
    clean
        .charge("t0", usage(1_000, 100), GPT_5_6_LUNA, budget)
        .expect("charged");
    assert!(!clean.any_may_understate());
}

/// No per-task figure and no projection before anything is charged.
#[test]
fn a_ledger_that_charged_nothing_projects_nothing() {
    let dir = tempfile::tempdir().expect("tmp");
    let l = fresh_ledger(&dir);
    assert_eq!(l.per_task(), None);
    assert_eq!(l.projected_for(241), None);
    assert_eq!(l.spent(), 0);
}

/// **The enforced cap and the reported authorisation are the same number, and
/// both sit inside the credit.**
///
/// This test used to open `assert_eq!(b.authorised, 8 * USD)` — and so it
/// passed, every run, for the entire life of a figure that had stopped being
/// true. A test that pins the number it polices polices nothing; it only
/// records what someone typed. So every assertion here is a RELATIONSHIP, and
/// the numbers are free to change without anyone having to remember to edit a
/// second copy of them.
#[test]
fn the_cap_is_the_authorisation_and_the_authorisation_is_inside_the_credit() {
    let b = Budget::pilot_authorisation();
    // The guard must never permit a dollar that was not authorised...
    assert!(
        b.run_total <= b.authorised,
        "the run cap {} permits more than the {} authorised",
        b.run_total,
        b.authorised
    );
    // ...and must never REPORT an authorisation the run could not reach. This
    // is the direction that was broken: $8.000000 printed over a $3 cap.
    assert!(
        b.authorised <= b.run_total,
        "`authorised` {} is money the guard would refuse to spend — the report \
         line and the enforced ceiling have drifted apart",
        b.authorised
    );
    // Nothing is authorised that does not exist.
    assert!(b.credit >= b.authorised);
    assert!(b.per_task < b.run_total);
    // The reserved remainder is the credit that this run may NOT touch.
    assert_eq!(b.reserved(), b.credit - b.authorised);
    assert!(
        b.reserved() > 0,
        "the whole credit is authorised, so nothing is held back"
    );
    // A second pilot would fit in the credit a first one leaves — after
    // someone authorises it. Against `credit`, because `authorised` is now the
    // cap itself and `authorised - run_total` is zero by construction.
    assert!(b.credit - b.run_total >= b.run_total, "no room to re-pilot");
    // Spending nothing leaves the whole authorisation and the whole credit.
    assert_eq!(b.authorised_remaining(0), b.authorised);
    assert_eq!(b.credit_remaining(0), b.credit);
    // And neither goes negative when the ledger somehow exceeds them.
    assert_eq!(b.authorised_remaining(b.credit * 2), 0);
    assert_eq!(b.credit_remaining(b.credit * 2), 0);
}

/// **The key lands in a file inside the container, and the scan must cover it.**
///
/// Terminal-Bench's codex setup script writes
/// `{"OPENAI_API_KEY": "..."}` to `$HOME/.codex/auth.json` in the container, in
/// addition to putting it in the environment. So a credential artifact is not a
/// hypothetical produced by a mistake — the harness creates one by design, and
/// anything that collects container files can pick it up.
#[test]
fn the_scan_covers_the_auth_file_the_harness_writes() {
    let detector = thruline_core::redact::resembles_known_credential;
    let planted = format!("sk-proj-{}", "TLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55");
    let artifacts = vec![
        (
            "container/root/.codex/auth.json".to_string(),
            format!("{{\n  \"OPENAI_API_KEY\": \"{planted}\"\n}}"),
        ),
        (
            "container/root/.codex/sessions/2026/09/03/rollout.jsonl".to_string(),
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1200}}".to_string(),
        ),
    ];
    let found = scan_artifacts(&artifacts, detector);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].artifact, "container/root/.codex/auth.json");
    // The rollout file — the thing we actually want to keep — is clean.
    assert!(detector(&planted));
}

/// **No script that touches the key may enable shell tracing.**
///
/// `set -x` prints every command it runs, including `export
/// OPENAI_API_KEY=…` — so a single trace flag turns the one file the operator
/// keeps at mode 600 into a line in a log. This walks every shell script in the
/// repository and fails if one both references the credential and enables
/// tracing.
///
/// It binds before the runner script exists, which is the point: the constraint
/// should already be enforced when someone writes it.
#[test]
fn no_script_that_touches_the_credential_enables_shell_tracing() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name == "target" || name == ".git" || name.starts_with("target") {
                continue;
            }
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "sh")
                || name.ends_with(".sh.j2")
                || name.ends_with(".bash")
            {
                out.push(p);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let mut scripts = Vec::new();
    walk(&root, &mut scripts);
    assert!(
        scripts.len() >= 3,
        "the walk found {} shell scripts",
        scripts.len()
    );

    let mut offenders = Vec::new();
    for p in &scripts {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        let touches_credential = text.contains("OPENAI_API_KEY")
            || text.contains("ANTHROPIC_API_KEY")
            || text.contains("thruline-bench/openai.env");
        if !touches_credential {
            continue;
        }
        let running: String = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        for trace in ["set -x", "set -o xtrace", "set -ex", "set -eux"] {
            if running.contains(trace) {
                offenders.push(format!("{}: {trace}", p.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these scripts touch a credential AND enable shell tracing, which prints \
         the export line:\n  {}",
        offenders.join("\n  ")
    );

    // Positive control: the detector recognises tracing when it is present.
    let poisoned = "#!/bin/sh\nset -eux\nexport OPENAI_API_KEY=x\n";
    let running: String = poisoned
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(running.contains("set -eux") && running.contains("OPENAI_API_KEY"));
}

// ---------------------------------------------------------------------------
// Stop after task 1.
// ---------------------------------------------------------------------------

use thruline_bench::pilot::{check_first_task, FirstTask};

fn good_first() -> FirstTask {
    FirstTask {
        task_id: "t0".into(),
        invocation_succeeded: true,
        rollout_file_found: true,
        rollout_parsed: true,
        usage: Some(usage(300_000, 60_000)),
        cost: 132_000,
    }
}

/// A healthy task 1 lets the run continue.
#[test]
fn a_clean_first_task_permits_the_other_nine() {
    let cost = check_first_task(&good_first(), 132_000, 3).expect("clean");
    assert_eq!(cost, 132_000);
}

/// **Fails closed on every condition**, and the message names which one.
///
/// Written as explicit cases rather than a table so a failure says which
/// condition stopped binding.
#[test]
fn every_first_task_condition_is_load_bearing() {
    let mut broken = good_first();
    broken.invocation_succeeded = false;
    let e = check_first_task(&broken, 132_000, 3).expect_err("must halt");
    assert!(e.to_string().contains("model slug"), "{e}");

    let mut broken = good_first();
    broken.usage = None;
    let e = check_first_task(&broken, 132_000, 3).expect_err("must halt");
    assert!(e.to_string().contains("inferred"), "{e}");

    let mut broken = good_first();
    broken.usage = Some(Usage::from_claude_code(
        1,
        1,
        0,
        0,
        0,
        UsageSource::TurnCompleted,
    ));
    let e = check_first_task(&broken, 132_000, 3).expect_err("must halt");
    assert!(e.to_string().contains("floor"), "{e}");

    let mut broken = good_first();
    broken.cost = 132_000 * 10;
    let e = check_first_task(&broken, 132_000, 3).expect_err("must halt");
    assert!(e.to_string().contains("projected"), "{e}");

    // And the clean one still passes, so the checks are about the mutations.
    check_first_task(&good_first(), 132_000, 3).expect("clean");
}

/// **The scan cannot miss because the file moved.**
///
/// The concern is real — a control that passes because the artifact is not
/// where it looked is the base64-walk lesson in a fourth doorway. The structural
/// answer is that `scan_artifacts` filters on **content**, never on path: the
/// same body is found at a host path, a materialised container path, a
/// docker-cp'd path, or a name nobody anticipated.
#[test]
fn the_credential_scan_is_path_independent() {
    let detector = thruline_core::redact::resembles_known_credential;
    let planted = format!("sk-proj-{}", "TLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55");
    let body = format!("{{\"OPENAI_API_KEY\": \"{planted}\"}}");

    for path in [
        "/root/.codex/auth.json",
        "container/root/.codex/auth.json",
        "runs/2026-09-03/task-001/container-fs/root/.codex/auth.json",
        "/var/lib/docker/overlay2/abc123/diff/root/.codex/auth.json",
        "an-unexpected-name",
        "",
    ] {
        let found = scan_artifacts(&[(path.to_string(), body.clone())], detector);
        assert_eq!(
            found.len(),
            1,
            "the scan missed a credential at `{path}` — it is looking at paths"
        );
    }

    // And a path that *looks* like the auth file but holds nothing is not
    // flagged: the scan is not matching on the name either.
    let decoy = scan_artifacts(&[("/root/.codex/auth.json".into(), "{}".into())], detector);
    assert!(
        decoy.is_empty(),
        "the scan flagged a path rather than a value"
    );
}

// ---------------------------------------------------------------------------
// The money guards, after the audit that found them decorative.
//
// Every test below fails against the code as it stood: the ceiling was a $3.50
// tripwire, the ledger was a `pub` field on a `Default` struct with no file
// behind it, `Price` had two rates, and the two price tables agreed only by
// hand. A guard never observed refusing is not a guard.
// ---------------------------------------------------------------------------

/// **The cap is $3.00, not $3.00 plus one task.**
///
/// `charge` added the cost and then compared, so a run could land anywhere up
/// to `run_total + per_task` while the doc comment said `run_total`. Two
/// separate things now hold the line: the charge is refused rather than applied
/// when it would cross, and `headroom_for_one_more` refuses *before* the model
/// is called when a worst-case task would not fit.
#[test]
fn the_run_ceiling_is_three_dollars_and_not_three_fifty() {
    let budget = Budget::pilot_authorisation();
    let dir = tempfile::tempdir().expect("tmp");
    let mut ledger = fresh_ledger(&dir);

    // $0.48 a task: legal per-task, and six of them leave $0.12 of room.
    let each = Usage::from_claude_code(0, 96_000, 0, 0, 0, UsageSource::Reconciled);
    assert_eq!(each.cost(CLAUDE_HAIKU_4_5), 480_000);
    for i in 0..6 {
        ledger
            .charge(&format!("t{i}"), each, CLAUDE_HAIKU_4_5, budget)
            .expect("under both caps");
    }
    assert_eq!(ledger.spent(), 2_880_000);

    // A $0.40 task. Under the per-task cap; it would take the total to $3.28.
    let crosses = Usage::from_claude_code(0, 80_000, 0, 0, 0, UsageSource::Reconciled);
    assert!(crosses.cost(CLAUDE_HAIKU_4_5) < budget.per_task);
    match ledger.charge("crosses", crosses, CLAUDE_HAIKU_4_5, budget) {
        Err(BudgetStop::RunTotal {
            spent, would_be, ..
        }) => {
            assert_eq!(spent, 2_880_000);
            assert_eq!(would_be, 3_280_000);
        }
        other => panic!("the run cap did not hold: {other:?}"),
    }
    // Not recorded. The OLD code would leave $3.28 here.
    assert_eq!(ledger.spent(), 2_880_000);
    assert!(ledger.spent() <= budget.run_total);

    // And the pre-flight refuses too, because $2.88 + a worst-case $0.50 is
    // $3.38. Refusing before the model runs is the only free refusal.
    match ledger.headroom_for_one_more(budget) {
        Err(BudgetStop::NoHeadroom { spent, cap, .. }) => {
            assert_eq!(spent, 2_880_000);
            assert_eq!(cap, 3 * USD);
        }
        other => panic!("a worst-case task was allowed to start: {other:?}"),
    }
}

/// **Spend outlives the process, because the runner is one process per task.**
///
/// `scripts/pilot-run.sh` invokes `tb run` once. Ten tasks were ten processes,
/// each opening a `Ledger::default()` at $0, so a "$3 run cap" was a $3 cap ten
/// times over. Dropping the value here stands in for the process exiting.
#[test]
fn spend_survives_a_process_restart() {
    let budget = Budget::pilot_authorisation();
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("spend.tsv");
    let each = Usage::from_claude_code(0, 50_000, 0, 0, 0, UsageSource::Reconciled);

    for i in 0..6 {
        // A NEW ledger every iteration: this is what the runner does.
        let mut l = Ledger::open(&path).expect("open");
        l.charge(&format!("t{i}"), each, CLAUDE_HAIKU_4_5, budget)
            .expect("under the caps");
        drop(l);
    }
    let reopened = Ledger::open(&path).expect("open");
    assert_eq!(reopened.spent(), 6 * 250_000, "spend did not survive");
    assert_eq!(reopened.tasks_charged(), 6);

    // The provenance survives too: a floor that came back looking like a
    // measurement is the failure this flag exists to prevent.
    let mut l = Ledger::open(&path).expect("open");
    l.charge(
        "floor",
        Usage::from_claude_code(0, 1_000, 0, 0, 0, UsageSource::TurnCompleted),
        CLAUDE_HAIKU_4_5,
        budget,
    )
    .expect("charged");
    drop(l);
    assert!(
        Ledger::open(&path).expect("open").any_may_understate(),
        "a restart forgot that the total is a floor"
    );

    // A corrupt line is a hard error, never a silent $0.
    std::fs::write(&path, "this is not a spend record\n").expect("write");
    let err = Ledger::open(&path).expect_err("a corrupt ledger must not read as zero");
    assert!(err.to_string().contains("expected 4"), "{err}");
}

/// **The `RunManifest::first()` lesson, applied to the money.**
///
/// A second, easier constructor IS the bypass. `Ledger` had `#[derive(Default)]`
/// and a `pub spent`, so `ledger.spent = 0` erased the record in one statement
/// and `Ledger::default()` produced one with no file behind it at all. This
/// reads the source, the same way `the_credential_check_cannot_hold_the_value`
/// does, because the property is about what the type *offers* a caller.
#[test]
fn the_money_ledger_has_exactly_one_door() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pilot.rs"),
    )
    .expect("pilot.rs");
    let at = src.find("pub struct Ledger {").expect("Ledger");
    let decl = &src[at..at + 200];
    for banned in ["pub spent", "pub tasks_charged", "pub any_may_understate"] {
        assert!(
            !decl.contains(banned),
            "Ledger exposes `{banned}` — one statement resets the recorded spend"
        );
    }
    // The derive line immediately above the struct must not grant `Default`.
    let derive_at = src[..at].rfind("#[derive").expect("derive");
    let derive = &src[derive_at..at];
    assert!(
        !derive.contains("Default"),
        "Ledger derives Default, which is a ledger with no file behind it: {derive}"
    );
    // And `open` is the only constructor that returns one.
    assert_eq!(
        src.matches("-> std::io::Result<Ledger>").count(),
        1,
        "more than one way to obtain a Ledger"
    );
}

/// **A two-rate price model cannot express the bill it is guarding.**
///
/// These token counts are not invented: they are the measured totals of a real
/// 533-message Claude Code transcript in `~/.claude/projects` on the machine
/// this was written on. Cache reads alone are three quarters of the spend, and
/// the two buckets the old `Price` had no field for are 91% of it.
#[test]
fn the_buckets_the_old_price_ignored_are_most_of_a_real_bill() {
    let measured = Usage::from_claude_code(
        1_066,
        319_286,
        141_999_444,
        0,
        1_532_602,
        UsageSource::Reconciled,
    );
    let full = measured.cost(CLAUDE_HAIKU_4_5);
    assert_eq!(full, 18_862_645, "the five-rate bill moved");

    // What a two-rate reader would have seen.
    let two_rate = Usage::from_claude_code(1_066, 319_286, 0, 0, 0, UsageSource::Reconciled)
        .cost(CLAUDE_HAIKU_4_5);
    assert_eq!(two_rate, 1_597_496);
    assert!(
        full / two_rate >= 11,
        "the understatement was {}x, not the measured 11.8x",
        full / two_rate
    );

    // And a $0.50 per-task cap: the real usage blows through it, the two-rate
    // reading does not. That is the difference between a guard and a decoration.
    let budget = Budget::pilot_authorisation();
    assert!(full > budget.per_task);
    assert!(two_rate > budget.per_task); // both trip here, but...

    // ...at a tenth the size the two-rate reading passes and the real one halts.
    let tenth =
        Usage::from_claude_code(106, 31_928, 14_199_944, 0, 153_260, UsageSource::Reconciled);
    assert!(tenth.cost(CLAUDE_HAIKU_4_5) > budget.per_task);
    let tenth_two_rate = Usage::from_claude_code(106, 31_928, 0, 0, 0, UsageSource::Reconciled)
        .cost(CLAUDE_HAIKU_4_5);
    assert!(
        tenth_two_rate < budget.per_task,
        "the two-rate reading also halted, so this comparison shows nothing"
    );
}

/// The Anthropic rates are the documented multipliers on base input, not four
/// numbers that happen to sit near them.
///
/// Read off <https://platform.claude.com/docs/en/about-claude/pricing> on
/// 2026-09-03: cache hit 0.1x, 5 m cache write 1.25x, 1 h cache write 2x. A
/// transcription error in any one of them shows up here as arithmetic rather
/// than as a wrong bill six weeks later.
#[test]
fn the_anthropic_rates_are_the_documented_multipliers() {
    let p = CLAUDE_HAIKU_4_5;
    assert_eq!(p.input_per_mtok, 1_000_000, "$1 / MTok base input");
    assert_eq!(p.output_per_mtok, 5_000_000, "$5 / MTok output");
    assert_eq!(p.cache_read_per_mtok * 10, p.input_per_mtok, "0.1x");
    assert_eq!(p.cache_write_5m_per_mtok * 4, p.input_per_mtok * 5, "1.25x");
    assert_eq!(p.cache_write_1h_per_mtok, p.input_per_mtok * 2, "2x");
    // The alias and the pinned snapshot are the same rate card.
    assert_eq!(price_for("claude-haiku-4-5"), Some(p));
    assert_eq!(price_for("claude-haiku-4-5-20251001"), Some(p));
    // And an unpriced model is a refusal, not a default.
    assert_eq!(price_for("claude-opus-5"), None);
    assert_eq!(price_for("gpt-5.6-sol"), None);
    assert_eq!(price_for(""), None);
}

/// **The two price tables cannot drift apart.**
///
/// The rates live in Rust (the guard) and in Python (the report). Two tables
/// that must agree, with only a comment saying so, is exactly how four of six
/// transcribed vendor facts went stale last round. This parses the Python
/// literal and compares.
#[test]
fn the_two_price_tables_cannot_drift_apart() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let py = std::fs::read_to_string(root.join("scripts/pilot-report.py")).expect("report");
    let at = py.find("PRICES = {").expect("PRICES literal");
    let end = at + py[at..].find("\n}").expect("close");
    // Python writes `1_000_000`; Rust's `{}` writes `1000000`. Drop the digit
    // separators so the comparison is about the NUMBER and not its spelling.
    let raw = &py[at..end];
    let chars: Vec<char> = raw.chars().collect();
    let mut body = String::with_capacity(raw.len());
    for (i, c) in chars.iter().enumerate() {
        let between_digits = *c == '_'
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if !between_digits {
            body.push(*c);
        }
    }
    let body = body.as_str();

    let mut seen = 0;
    for p in PRICES {
        let want = format!(
            "\"{}\": ({}, {}, {}, {}, {})",
            p.model,
            p.input_per_mtok,
            p.output_per_mtok,
            p.cache_read_per_mtok,
            p.cache_write_5m_per_mtok,
            p.cache_write_1h_per_mtok
        );
        assert!(
            body.contains(&want),
            "scripts/pilot-report.py disagrees with crates/bench/src/pilot.rs about \
             `{}`. Expected the row:\n  {want}\nGot:\n{body}",
            p.model
        );
        seen += 1;
    }
    assert!(seen >= 2, "only {seen} model(s) cross-checked");

    // Positive control: the comparison is capable of failing.
    let bogus = Price {
        model: "not-a-model",
        input_per_mtok: 1,
        output_per_mtok: 1,
        cache_read_per_mtok: 1,
        cache_write_5m_per_mtok: 1,
        cache_write_1h_per_mtok: 1,
    };
    assert!(!body.contains(&format!("\"{}\": (1, 1, 1, 1, 1)", bogus.model)));
}

/// **The two vendors disagree about whether `input_tokens` includes the cache,
/// and getting it backwards is a silent factor of two.**
///
/// Codex reports input INCLUSIVE of cached input; Claude Code reports it
/// EXCLUSIVE. Both constructors exist so that neither convention can be applied
/// to the other vendor's numbers by writing a struct literal.
#[test]
fn the_two_vendor_token_conventions_are_opposite_and_both_are_handled() {
    // Codex: 100k input of which 80k was cached. Base-rate input is 20k.
    let codex = Usage::from_codex(100_000, 80_000, 5_000, UsageSource::RolloutFile);
    assert_eq!(codex.input_tokens, 20_000);
    assert_eq!(codex.cache_read_input_tokens, 80_000);
    assert_eq!(codex.billable_tokens(), 105_000, "no token double-counted");
    // Codex bills no cache-write line; zero by construction, not by omission.
    assert_eq!(codex.cache_write_5m_input_tokens, 0);
    assert_eq!(codex.cache_write_1h_input_tokens, 0);

    // A pathological record where cached exceeds input cannot go negative or
    // wrap: saturating, and the cache bucket is clamped.
    let odd = Usage::from_codex(10, 999, 1, UsageSource::RolloutFile);
    assert_eq!(odd.input_tokens, 0);
    assert_eq!(odd.cache_read_input_tokens, 10);

    // Claude Code: already disjoint, passed through.
    let cc = Usage::from_claude_code(20_000, 5_000, 80_000, 1_000, 2_000, UsageSource::Reconciled);
    assert_eq!(cc.input_tokens, 20_000);
    assert_eq!(cc.billable_tokens(), 108_000);
}

/// **The runner gates the credential that matches the agent, and the model gate
/// is an allowlist.**
///
/// `scripts/pilot-run.sh` demanded `OPENAI_API_KEY` for every non-oracle agent
/// and never mentioned `ANTHROPIC_API_KEY`, so a `claude-code` run passed the
/// gate on a key it would never use and then ran unauthenticated — producing
/// empty output that reads downstream as a coverage finding. And the model gate
/// was `[ -z "$MODEL" ]`: any non-empty string, frontier models included.
#[test]
fn the_runner_gates_the_credential_that_matches_the_agent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let sh = std::fs::read_to_string(root.join("scripts/pilot-run.sh")).expect("runner");
    let running: String = sh
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        running.contains("ANTHROPIC_API_KEY"),
        "the runner never checks the Anthropic credential"
    );
    assert!(running.contains("OPENAI_API_KEY"));
    // An unknown agent must be refused, not handed OpenAI's door by default.
    assert!(
        running.contains("unknown agent"),
        "an unrecognised agent still falls through to a default credential"
    );
    // The model gate is an allowlist.
    assert!(
        running.contains("MODELS_OK") && running.contains("not on the allowlist"),
        "the model gate is not an allowlist"
    );
    // Every allowlisted model must be one this build can actually price — an
    // unpriced run cannot be charged, and what cannot be charged cannot be
    // capped.
    let at = running
        .find("MODELS_OK=\"claude")
        .expect("claude allowlist");
    let line = &running[at..at + 80];
    for m in ["claude-haiku-4-5", "claude-haiku-4-5-20251001"] {
        assert!(line.contains(m), "{line}");
        assert!(price_for(m).is_some(), "`{m}` is allowlisted but unpriced");
    }
    // And the ceiling is actually invoked, which it was not: `grep -c pilot
    // crates/cli/src/main.rs` returned 0 before this change.
    assert!(
        running.contains("pilot-budget --ledger") && running.contains("check"),
        "the runner never asks the budget guard anything"
    );
    assert!(
        running.contains("charge"),
        "the runner never records what it spent"
    );

    let main = std::fs::read_to_string(root.join("crates/cli/src/main.rs")).expect("main.rs");
    assert!(
        main.contains("PilotBudget"),
        "the guard is still unreachable from the binary"
    );
}

/// **A halt that returns success is not a halt.**
///
/// `scripts/pilot-report.py` printed "Per pilot::check_first_task this HALTS the
/// run" and exited 0, so the runner read the status, saw zero and carried on.
/// It also read `d["usage"]`, which on a real 8,187-line Codex rollout matched
/// **0** lines while `payload.info.total_token_usage` matched 1,693.
#[test]
fn the_report_reads_the_field_that_exists_and_exits_non_zero_on_a_halt() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let py = std::fs::read_to_string(root.join("scripts/pilot-report.py")).expect("report");

    assert!(
        py.contains("total_token_usage"),
        "the report still reads a usage field the rollout does not carry"
    );
    assert!(
        py.contains("cache_read_input_tokens") && py.contains("cache_creation_input_tokens"),
        "the report ignores the two buckets that were 91% of a real bill"
    );
    assert!(
        py.contains("sys.exit(5)") && py.contains("HALT_REASONS"),
        "the report can still print HALT and exit 0"
    );
    // The cumulative reduction, not a sum: summing overstated the real rollout
    // by 820x on input.
    assert!(
        py.contains("_cumulative_total"),
        "the report sums a cumulative counter"
    );
}

// ---------------------------------------------------------------------------
// The ledger under concurrency.
//
// `scripts/pilot-run.sh` runs one `thruline pilot-budget charge` per task, and
// nothing stops two of them overlapping — a parallel `tb run`, a retry, or a
// human typing the subcommand while a run is in flight. `pilot-budget` is a
// public subcommand, so a lock in the shell script would guard nothing.
//
// Measured against the code as it stood, with five concurrent charges from the
// real binary: **9 of 10 trials shredded the ledger** into lines like
// `t5\tt4t3300000\t\tt1claude-haiku-4-5-20251001…`, after which `show` and
// `check` exit 1 with "spend ledger line has 13 fields, expected 4" — the
// record of what was spent is destroyed. And in the same 9 of 10 trials, every
// one of the five charges was ACCEPTED against a ledger with room for one,
// because each had loaded its total before any of them appended.
// ---------------------------------------------------------------------------

/// **Five concurrent charges, room for one. Exactly one lands.**
///
/// Threads rather than processes because there is nothing in-process to share:
/// each one opens its own `Ledger` from the file, exactly as a fresh
/// `pilot-budget charge` invocation does, and the only thing serialising them
/// is on the filesystem. Reproduced identically with five real processes.
#[test]
fn concurrent_charges_admit_exactly_one_and_leave_a_readable_ledger() {
    let budget = Budget::pilot_authorisation();
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("spend.tsv");
    // Loaded to `run_total - per_task`: $2.50 of the $3.00 cap.
    std::fs::write(&path, "seed\t2500000\tclaude-haiku-4-5\treconciled\n").expect("seed");

    const N: usize = 5;
    // $0.30 each: one fits under the $3.00 cap, two do not.
    let each = Usage::from_claude_code(0, 60_000, 0, 0, 0, UsageSource::Reconciled);
    assert_eq!(each.cost(CLAUDE_HAIKU_4_5), 300_000);

    let gate = std::sync::Arc::new(std::sync::Barrier::new(N));
    let mut spawned = Vec::with_capacity(N);
    for i in 0..N {
        let path = path.clone();
        let gate = std::sync::Arc::clone(&gate);
        spawned.push(std::thread::spawn(move || {
            // What one `pilot-budget charge` invocation does: read the file,
            // charge, exit. The read is deliberately BEFORE the barrier, which
            // is the whole hazard — the total each caller compares against is
            // already stale by the time it compares.
            let mut ledger = Ledger::open(&path).expect("open");
            gate.wait();
            ledger.charge(&format!("t{i}"), each, CLAUDE_HAIKU_4_5, budget)
        }));
    }
    let outcomes: Vec<Result<_, BudgetStop>> = spawned
        .into_iter()
        .map(|h| h.join().expect("a charge panicked"))
        .collect();

    let accepted = outcomes.iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        accepted, 1,
        "{accepted} of {N} concurrent charges were accepted against a cap with \
         room for one. Every extra one is real money spent past the ceiling: \
         {outcomes:?}"
    );
    for outcome in &outcomes {
        match outcome {
            Ok(_) => {}
            // `exit 4` in the CLI is exactly this: a budget halt, and nothing
            // else. A refusal for any other reason would be an outage, not a cap.
            Err(BudgetStop::RunTotal {
                spent,
                would_be,
                cap,
                ..
            }) => {
                assert_eq!(*cap, 3 * USD);
                assert!(spent <= cap, "a halt reported spend past the cap");
                assert!(would_be > cap);
            }
            other => panic!("a concurrent charge was refused for the wrong reason: {other:?}"),
        }
    }

    // The record survives, and says what was spent.
    let raw = std::fs::read_to_string(&path).expect("read");
    assert_eq!(
        raw.lines().count(),
        2,
        "the ledger holds {} lines, not the seed plus one charge:\n{raw}",
        raw.lines().count()
    );
    let reopened = Ledger::open(&path).expect("the concurrent charges shredded the ledger");
    assert_eq!(reopened.tasks_charged(), 2);
    assert_eq!(reopened.spent(), 2_800_000);
    assert!(reopened.spent() <= budget.run_total);
}

/// **Concurrent charges that are all legal all land, whole.**
///
/// The refusal test above could be passed by a lock that simply lost four
/// records. This one has room for every charge, so the ledger must end with all
/// of them, each on one well-formed line, and the total must be exact.
#[test]
fn concurrent_charges_that_all_fit_are_all_recorded_intact() {
    let budget = Budget::pilot_authorisation();
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("spend.tsv");

    const N: usize = 8;
    // $0.05 each: $0.40 in total, far under both caps.
    let each = Usage::from_claude_code(0, 10_000, 0, 0, 0, UsageSource::Reconciled);
    assert_eq!(each.cost(CLAUDE_HAIKU_4_5), 50_000);

    let gate = std::sync::Arc::new(std::sync::Barrier::new(N));
    let mut spawned = Vec::with_capacity(N);
    for i in 0..N {
        let path = path.clone();
        let gate = std::sync::Arc::clone(&gate);
        spawned.push(std::thread::spawn(move || {
            let mut ledger = Ledger::open(&path).expect("open");
            gate.wait();
            ledger.charge(&format!("task-{i:02}"), each, CLAUDE_HAIKU_4_5, budget)
        }));
    }
    for h in spawned {
        h.join().expect("a charge panicked").expect("all fit");
    }

    let raw = std::fs::read_to_string(&path).expect("read");
    assert_eq!(raw.lines().count(), N, "interleaved records:\n{raw}");
    for (n, line) in raw.lines().enumerate() {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 4, "line {} is not one record: {line:?}", n + 1);
        assert_eq!(f[1], "50000", "line {} carries a spliced cost", n + 1);
    }
    let mut ids: Vec<&str> = raw.lines().map(|l| l.split('\t').next().unwrap()).collect();
    ids.sort_unstable();
    let want: Vec<String> = (0..N).map(|i| format!("task-{i:02}")).collect();
    assert_eq!(ids, want, "a task id was lost or forged");

    let reopened = Ledger::open(&path).expect("open");
    assert_eq!(reopened.spent(), N as u64 * 50_000);
    assert_eq!(reopened.tasks_charged(), N);
}

// ---------------------------------------------------------------------------
// The committed Terminal-Bench split (Ruling 5).
//
// These tests ARE the re-derivation the manifest names in its own header. A
// committed constant with no re-derivation is a liability; the constant here is
// 241 assignments and a 93-name holdout, and each test below is a way it could
// go stale silently.
// ---------------------------------------------------------------------------

use thruline_bench::pilot::{ObservedTier, SplitError, TerminalBenchSplit, TERMINAL_BENCH_SPLIT};

#[test]
fn holdout_manifest_loads_and_re_derives_every_assignment() {
    let s = TerminalBenchSplit::load().expect("the committed split must parse");
    // `parse` recomputes `bucket(salt, id) < cut` for all 241 rows and refuses
    // any disagreement, so reaching this line is the re-derivation passing.
    assert_eq!(s.corpus_size(), 241);
    assert_eq!(s.salt(), "thruline-bench-holdout-v1");
    assert_eq!(s.cut(), 1u64 << 63);
    // The measured split, not a target: 118 of 241 land on the holdout side.
    assert_eq!(s.holdout_size() + s.observed_in_holdout_side(), 118);
    // 93, not 94: `ancient-puzzle` was spent 27 minutes after this file was
    // committed and now carries a run-record row. A holdout that was run is
    // gone, and the repair is the `observed` list — its side column is a hash
    // fact and `AssignmentDrift` refuses an edit to it.
    assert_eq!(s.holdout_size(), 93);
    assert_eq!(s.tune().len(), 241 - 93);
}

#[test]
fn holdout_manifest_every_exclusion_carries_a_record() {
    let s = TerminalBenchSplit::load().expect("parse");
    assert_eq!(s.observed().len(), 56);
    for o in s.observed() {
        assert!(
            !o.record.trim().is_empty(),
            "`{}` is excluded with no record — the exact thing Ruling 5 forbids",
            o.task
        );
        assert!(!o.note.trim().is_empty(), "`{}` has no note", o.task);
        assert!(
            !o.fingerprint.trim().is_empty(),
            "`{}` has no fingerprint",
            o.task
        );
    }
    let exec = s
        .observed()
        .iter()
        .filter(|o| o.tier.is_execution())
        .count();
    assert_eq!(
        exec, 38,
        "the record-derived count under the narrow definition"
    );
    assert_eq!(
        s.observed()
            .iter()
            .filter(|o| o.tier == ObservedTier::RunRecord)
            .count(),
        35
    );
}

#[test]
fn holdout_manifest_a_hand_edited_assignment_does_not_load() {
    // Flip one `task` row's side. The hash says otherwise, so the file refuses
    // to become a split. This is what makes the constant self-checking rather
    // than merely documented.
    let flipped = TERMINAL_BENCH_SPLIT.replacen(
        "task\tacl-permissions-inheritance\tholdout",
        "task\tacl-permissions-inheritance\ttune",
        1,
    );
    assert_ne!(flipped, TERMINAL_BENCH_SPLIT, "fixture row not found");
    match TerminalBenchSplit::parse(&flipped) {
        Err(SplitError::AssignmentDrift { task, .. }) => {
            assert_eq!(task, "acl-permissions-inheritance")
        }
        other => panic!("a hand-edited assignment loaded: {other:?}"),
    }
}

#[test]
fn holdout_manifest_a_padded_exclusion_list_does_not_load() {
    // Adding a name without also moving the counts is how a list gets padded to
    // look complete. The header and the body must agree.
    let padded = format!(
        "{TERMINAL_BENCH_SPLIT}observed\tacl-permissions-inheritance\trepo-named\tsomewhere\tgit-tracked\tplausible\n"
    );
    match TerminalBenchSplit::parse(&padded) {
        Err(SplitError::CountDrift { field, .. }) => assert_eq!(field, "observed_total"),
        other => panic!("a padded exclusion list loaded: {other:?}"),
    }
}

#[test]
fn holdout_manifest_a_nameless_exclusion_does_not_load() {
    let nameless = TERMINAL_BENCH_SPLIT.replacen(
        "observed\tassign-seats\trepo-named\t",
        "observed\tassign-seats\trepo-named\t\t",
        1,
    );
    assert!(matches!(
        TerminalBenchSplit::parse(&nameless),
        Err(SplitError::Malformed { .. })
    ));
}

#[test]
fn holdout_manifest_digest_covers_the_ids_it_lists() {
    let dropped = TERMINAL_BENCH_SPLIT.replacen("task\twrite-compressor\tholdout\n", "", 1);
    assert_ne!(dropped, TERMINAL_BENCH_SPLIT, "fixture row not found");
    match TerminalBenchSplit::parse(&dropped) {
        Err(SplitError::CountDrift { field, .. }) => assert_eq!(field, "corpus_size"),
        Err(SplitError::DigestDrift { .. }) => {}
        other => panic!("a dropped task loaded: {other:?}"),
    }
}

/// **`#[ignore]`, for the same reason as `holdout_intersection.rs`'s sweep.**
///
/// This was a plain `#[test]` that printed "SKIPPED corpus comparison" and
/// returned when `THRULINE_TB_CORPUS` was unset — which it is in every gate — so
/// the harness counted it as a pass. Its name hedges (*"when one is present"*)
/// and the hedge is not enough: a green row says the check ran.
///
/// **Found by sweeping, not by suspecting.** The first instance of this shape in
/// this crate was found by suspecting it; this one was found afterwards by
/// grepping every `env::var` and every bare `return` in a test body. One method
/// found one, and would have left this. That is the same lesson as the six temp
/// roots one layer along.
#[ignore = "needs THRULINE_TB_CORPUS: the 241-task corpus is not in this repo.             Ignored rather than silently passing."]
#[test]
fn holdout_manifest_matches_the_corpus_on_disk_when_one_is_present() {
    // The half the compiled-in text cannot check by itself. Opt-in by env var
    // because the corpus is a 241-directory tree that is not in this repo — and
    // **loud** when it is present and disagrees, per Rule 12.
    let Some(dir) = std::env::var_os("THRULINE_TB_CORPUS") else {
        eprintln!(
            "SKIPPED corpus comparison: THRULINE_TB_CORPUS is unset. The split's \
             internal re-derivation still ran; what is NOT checked here is whether \
             the 241 ids still describe the corpus on disk."
        );
        return;
    };
    let s = TerminalBenchSplit::load().expect("parse");
    if let Err(e) = s.check_against_corpus(std::path::Path::new(&dir)) {
        panic!("{e}");
    }
}

#[test]
fn a_draw_from_tune_cannot_reach_the_holdout_because_the_ids_are_not_there() {
    // Not "the filter refuses it": the pool does not contain them. Take the
    // whole tune slice — the widest draw possible — and no holdout id appears.
    let s = TerminalBenchSplit::load().expect("parse");
    let ledger = tempfile::tempdir().expect("tmp");
    let mut hl = thruline_bench::HoldoutLedger::open(&ledger.path().join("spent")).expect("open");
    let m = hl.spend(&s.split_hash(), "audit").expect("spend");
    let held: std::collections::BTreeSet<&String> =
        s.holdout(&m).expect("released").iter().collect();
    for t in s.tune() {
        assert!(!held.contains(t), "`{t}` is in both tune and holdout");
    }
    assert_eq!(s.tune().len() + held.len(), s.corpus_size());
}

#[test]
fn reading_the_holdout_names_is_itself_a_recorded_spend() {
    let s = TerminalBenchSplit::load().expect("parse");
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("spent");

    // A manifest for some other protocol does not open this door.
    let mut other = thruline_bench::HoldoutLedger::open(&path).expect("open");
    let wrong = other.spend("sha256:not-this-split", "r0").expect("spend");
    assert!(matches!(
        s.holdout(&wrong),
        Err(SplitError::HoldoutNotReleased { .. })
    ));

    // The first real spend releases the names, and the file records it.
    let mut hl = thruline_bench::HoldoutLedger::open(&path).expect("open");
    let first = hl.spend(&s.split_hash(), "r1").expect("spend");
    assert_eq!(s.holdout(&first).expect("released").len(), 93);
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains(&format!("{}\tr1", s.split_hash())));

    // The second refuses, from the file rather than from memory.
    let mut again = thruline_bench::HoldoutLedger::open(&path).expect("open");
    let second = again.spend(&s.split_hash(), "r2").expect("spend");
    match s.holdout(&second) {
        Err(SplitError::HoldoutAlreadySpent { prior_runs }) => assert_eq!(prior_runs, vec!["r1"]),
        other => panic!("a second read of the holdout was allowed: {other:?}"),
    }
}

#[test]
fn the_empty_holdout_bypass_in_plan_is_closed() {
    // `RunManifest::first()`'s shape, one crate over: `plan` derived its pool
    // from `base.holdout`, so a base protocol with an empty holdout made the
    // pool the whole corpus — and `check_from_tune`, comparing against the same
    // empty set, agreed. One argument and the holdout is spent.
    let mut p = base(40);
    assert!(plan(&p, 3, Harness::ClaudeCode, &[]).is_ok());
    p.holdout = Vec::new();
    assert!(
        matches!(
            plan(&p, 3, Harness::ClaudeCode, &[]),
            Err(PilotError::UncommittedHoldout)
        ),
        "an empty holdout still yields a pool of the whole corpus"
    );
    // And the same shape through `tune_split` directly, which is what `plan`
    // called: the pool was the corpus, not a subset of it.
    assert_eq!(tune_split(&p.tasks, &[]).len(), p.tasks.len());
}

#[test]
fn a_protocol_seeded_from_the_split_carries_the_committed_holdout() {
    let s = TerminalBenchSplit::load().expect("parse");
    let seeded = s.tune_protocol(&base(40));
    assert_eq!(seeded.tasks.len(), s.tune().len());
    assert_eq!(seeded.holdout.len(), s.holdout_size());
    // A pilot planned from it draws only from tune, and every drawn id is one
    // the split calls tune.
    let pilot = plan(&seeded, 5, Harness::ClaudeCode, &[]).expect("plan");
    let tune: std::collections::BTreeSet<&String> = s.tune().iter().collect();
    for t in &pilot.tasks {
        assert!(tune.contains(t), "`{t}` was drawn and is not in tune");
    }
    check_from_tune(&pilot, &seeded).expect("a pilot from the split is from tune");
}
