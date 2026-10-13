//! BUILD-SPEC §6.3 — the `tests_pass` verifier.
//!
//! > *Supported iff: a test check exists with exit code 0, bound to the final
//! > tree (6.1), `tests_total > 0`, `failed == 0`, and the following detectors
//! > raise no `fail`-severity finding.*
//!
//! # It is a conjunction, and every clause is a way to say no
//!
//! The structure below is deliberately "start at `Unverifiable`, earn
//! `Supported`" rather than "start at `Supported`, lose it". A verifier written
//! the other way is one missing `else` from asserting a claim it never checked,
//! and §6's non-collapse rule is exactly the rule that mistake breaks.

use crate::binding::{bind, is_test_path, Binding, CheckView, ParsedView, SessionView};
use crate::verdict::{Finding, Severity, Verdict, VerdictRecord};

pub const VERIFIER: &str = "tests_pass@1";

/// Flags that narrow a run to a subset of the suite.
///
/// Matched on the **argv**, never on a command string — §13, and the same
/// reason `Runner::from_argv` exists.
const NARROWING_FLAGS: &[&str] = &[
    "-k",
    "-m",
    "-t",
    "--filter",
    "--testnamepattern",
    "--test-name-pattern",
    "--test-skip-pattern",
    "--test-only",
    "--test-shard",
    "--lf",
    "--last-failed",
    "--ff",
    "--failed-first",
    "--deselect",
    "--ignore",
    "--exact",
];

/// What the caller must supply beyond the session: the claim's scope, and any
/// §6.7 findings about test files changed in this session.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub scope_is_all: bool,
    pub tamper_findings: Vec<Finding>,
    /// Paths of non-test source files changed in the session. Used for §6.3's
    /// "test files modified without a corresponding source change" rule.
    pub source_changed: bool,
    pub test_files_changed: Vec<String>,
    /// §6.3's last clause. `None` when coverage was not computed at all —
    /// which is not the same as 0%, and must not read as it.
    pub coverage: Option<crate::coverage::Coverage>,
}

/// Verify a `tests_pass` claim.
pub fn verify(claim_id: &str, session: &SessionView, inputs: &Inputs) -> VerdictRecord {
    // §13: capture known incomplete can never reach `supported`.
    if session.capture_degraded {
        return VerdictRecord::new(claim_id, VERIFIER, Verdict::Unverifiable).reason(format!(
            "capture is degraded for this session ({}), so no claim in it can be supported",
            session
                .degraded_reason
                .clone()
                .unwrap_or_else(|| "reason not recorded".to_string())
        ));
    }

    // Every check in the view, unfiltered. Filtering on `parsed.is_some()`
    // dropped exactly the case §6.2's return type exists to preserve: a run
    // that exited 0 and whose output could not be read was excluded here and
    // then reported as "no test check", which reads as "the agent ran nothing"
    // instead of "the run's result is unknown". The caller supplies test
    // checks; selecting among them is this function's job, discarding them is
    // not.
    let test_checks: Vec<&CheckView> = session
        .checks
        .iter()
        .filter(|c| c.category == crate::check::CATEGORY_TEST)
        .collect();
    if test_checks.is_empty() {
        return VerdictRecord::new(claim_id, VERIFIER, Verdict::Unverifiable).reason(
            "no test check was captured in this session; the claim may be true and \
             there is no evidence either way",
        );
    }

    // A failing check is evidence AGAINST, and it outranks any passing one:
    // §6.3's whole point is that a green run must not cover a red one. A
    // re-execution that failed counts here too — §7 is explicit that a
    // mismatch is `unsupported` — but it is named as one, because "a captured
    // test run failed" about a container is a sentence that misplaces the fact.
    if let Some(failed) = test_checks
        .iter()
        .find(|c| c.exit_code != 0 || c.parsed.as_ref().is_some_and(|p| p.failed > 0))
    {
        let what = if failed.reexecution.is_some() {
            "a re-execution of a claimed test run failed at the final tree"
        } else {
            "a captured test run failed"
        };
        return VerdictRecord::new(claim_id, VERIFIER, Verdict::Unsupported)
            .reason(format!(
                "{what}: `{}` exited {}",
                failed.command(),
                failed.exit_code
            ))
            .with_finding(Finding::new(
                Severity::Fail,
                if failed.reexecution.is_some() {
                    "reexecution_failed"
                } else {
                    "test_run_failed"
                },
                format!("`{}` exited {}", failed.command(), failed.exit_code),
            ));
    }

    // **The selection reads captured checks only.**
    //
    // A re-executed row is not a candidate to *carry* the claim. `reexec`
    // stamps its own row with the session's final tree hash, so it satisfies
    // §6.1 rule 1 by construction and used to win this loop outright — which
    // made `thruline reexec` a verdict-upgrading operation: it turned a
    // correctly-superseded original into `supported` and dropped the
    // `tests_ran_before_final_edit` finding on the way. Re-executions are
    // folded in below, where they can confirm or contradict.
    let (captured, reexecutions): (Vec<&CheckView>, Vec<&CheckView>) = test_checks
        .iter()
        .copied()
        .partition(|c| c.reexecution.is_none());

    if captured.is_empty() {
        let mut record = VerdictRecord::new(claim_id, VERIFIER, Verdict::Unverifiable).reason(
            "the only test evidence in this session is a re-execution; a re-execution \
             confirms or contradicts a run this system captured, and there is no \
             captured run here for it to be about",
        );
        // Recorded even though it cannot count: §6.1 says to record WHICH rule
        // applied, and "a re-execution, which §6.1 would have called X" is the
        // rule that applied. A `null` here would hide that the row was ordered
        // at all.
        if let [only] = reexecutions.as_slice() {
            record.binding = Some(bind(session, only));
        }
        return fold_reexecutions(record, session, inputs, &reexecutions, None);
    }

    // Pick the best passing check: bound to the final tree if any is.
    let mut chosen: Option<(&CheckView, Binding)> = None;
    for check in &captured {
        let binding = bind(session, check);
        let better = match &chosen {
            None => true,
            Some((_, current)) => {
                matches!(binding, Binding::RanOnFinalTree)
                    && !matches!(current, Binding::RanOnFinalTree)
            }
        };
        if better {
            chosen = Some((check, binding));
        }
    }
    let Some((check, binding)) = chosen else {
        return VerdictRecord::new(claim_id, VERIFIER, Verdict::Unverifiable)
            .reason("no test check could be selected");
    };

    let counted = binding.counts();
    let record = verify_captured(claim_id, inputs, check, binding, &captured);
    fold_reexecutions(record, session, inputs, &reexecutions, Some(counted))
}

/// §6.3 over the session's **own** evidence: the captured check that carries
/// the claim, and the detectors that qualify it.
#[allow(clippy::too_many_lines)]
fn verify_captured(
    claim_id: &str,
    inputs: &Inputs,
    check: &CheckView,
    binding: Binding,
    captured: &[&CheckView],
) -> VerdictRecord {
    let test_checks = captured;
    let mut record = VerdictRecord::new(claim_id, VERIFIER, Verdict::Supported);
    record.evidence.push(check.id.clone());
    record.reasons.push(binding.describe());
    record.binding = Some(binding.clone());

    // --- binding (§6.1) ----------------------------------------------------
    if !binding.counts() {
        record.verdict = match binding {
            // Evidence for a tree that is not the final one is evidence
            // against the claim as stated, not an absence of evidence.
            Binding::SupersededByEdit { .. } | Binding::SupersededByUnobservedChange { .. } => {
                Verdict::Unsupported
            }
            _ => Verdict::Unverifiable,
        };
        if matches!(
            binding,
            Binding::SupersededByEdit { .. } | Binding::SupersededByUnobservedChange { .. }
        ) {
            record = record.with_finding(Finding::new(
                Severity::Fail,
                "tests_ran_before_final_edit",
                binding.describe(),
            ));
        }
        return record;
    }

    // --- the counts themselves --------------------------------------------
    let Some(parsed) = check.parsed.as_ref() else {
        record.verdict = Verdict::Unverifiable;
        record.reasons.push(
            "the runner's output could not be parsed, so the run's result is unknown — \
             which is not the same as passing"
                .to_string(),
        );
        return record;
    };
    if parsed.tests_total == 0 {
        record.verdict = Verdict::Unverifiable;
        record
            .reasons
            .push("the run reported zero tests; a suite that ran nothing proves nothing".into());
        return record;
    }
    record
        .reasons
        .push(format!("{}/{} passed", parsed.passed, parsed.tests_total));

    // --- narrowing (§6.3) --------------------------------------------------
    //
    // The command is read from the record. When the record does not hold one,
    // the detector cannot look — and says so, rather than producing the same
    // silence a full-suite run produces. That silence is what let a re-executed
    // `pytest -k` support a `scope: all` claim.
    match check.argv.as_deref() {
        Some(argv) if !argv.is_empty() => {
            if let Some(flag) = narrowing_flag(argv) {
                let detail = format!(
                    "`{}` narrows the run with `{flag}`, so it exercised a subset of the suite",
                    check.command()
                );
                // A narrowed run supports `named`/`changed` and never `all`.
                record = record.with_finding(Finding::new(
                    if inputs.scope_is_all {
                        Severity::Fail
                    } else {
                        Severity::Warn
                    },
                    "narrowed_run",
                    detail,
                ));
            }
        }
        _ => {
            record = record.with_finding(Finding::new(
                Severity::Warn,
                "narrowing_unchecked",
                "the command behind this check is not in the record, so §6.3's \
                 narrowing detector could not look at it — an absence of \
                 `narrowed_run` here is not evidence that the run was not narrowed",
            ));
            if inputs.scope_is_all {
                // `unverifiable`, never `unsupported`: nothing here contradicts
                // the claim. And never `supported`, because the clause §6.3
                // makes a precondition of `scope: all` was not evaluated.
                record.verdict = record.verdict.worst(Verdict::Unverifiable);
            }
        }
    }

    // --- flakiness (§6.3) --------------------------------------------------
    if let Some(detail) = nondeterministic(test_checks) {
        record = record.with_finding(Finding::new(Severity::Fail, "nondeterministic", detail));
    }

    // --- skips (§6.3) ------------------------------------------------------
    if parsed.skipped > 0 || parsed.xfail > 0 {
        record = record.with_finding(Finding::new(
            Severity::Warn,
            "skipped_tests",
            format!(
                "{} skipped and {} xfailed of {} — they were not run",
                parsed.skipped, parsed.xfail, parsed.tests_total
            ),
        ));
    }

    // --- test files changed without a source change (§6.3) -----------------
    if !inputs.test_files_changed.is_empty() && !inputs.source_changed {
        record = record.with_finding(Finding::new(
            Severity::Warn,
            "test_changed_without_source_change",
            format!(
                "test file(s) changed with no non-test source change to necessitate it: {}",
                inputs.test_files_changed.join(", ")
            ),
        ));
    }

    // --- coverage of changed lines (§6.3) ----------------------------------
    if let Some(cov) = &inputs.coverage {
        if let Some(finding) = cov.finding(crate::coverage::DEFAULT_MIN_PCT) {
            record = record.with_finding(finding);
        }
    }

    // --- tampering (§6.7), computed elsewhere and folded in here ------------
    //
    // Two of them are ESCALATED here rather than in the detector, because the
    // fact that decides them is in the execution record and the detector only
    // sees source. §0.2's own principle: prefer the record over the text
    // wherever both are available.
    for finding in &inputs.tamper_findings {
        record = record.with_finding(escalate(finding.clone(), parsed));
    }

    record
}

/// Fold §7's re-executions into a verdict reached from the session's own
/// evidence — **as confirmation or contradiction, never as promotion.**
///
/// > *"Re-execution beats trust. Any claimed check can be re-run in a clean
/// > environment at the final tree hash."* (§0 principle 4)
/// > *"Compare parsed results with the agent's run; mismatches are
/// > `unsupported` with both records attached."* (§7)
///
/// # The deviation this makes, stated rather than smuggled
///
/// §7 says verifiers *prefer* re-executed checks, and a permissive reading of
/// that sentence is what produced the defect: `reexec` inserted a row stamped
/// with the session's final tree hash, §6.1 rule 1 matched it by construction,
/// §6.3 preferred it, and a session correctly marked `unsupported` came back
/// `supported` — with its `tests_ran_before_final_edit` finding deleted from
/// the record rather than answered. Measured, not argued: the upgrade
/// reproduces end to end on a session whose only defect was that its tests ran
/// before its last source edit.
///
/// **A re-execution cannot carry a claim on its own**, for three reasons that
/// are properties of the mechanism and not of any particular session:
///
/// 1. It runs a **materialised git tree**, not the working directory the
///    session ended on — no untracked files, no ignored files, submodule
///    content absent. §6.1's `final_tree_hash` names the first; the claim is
///    about the second.
/// 2. It runs under a **different environment** (a container image, a minimal
///    env, a synthetic cwd), so "the same command passed" is a statement about
///    a different experiment.
/// 3. Its `input_tree_hash` is **written by the process that ran it**. Rule 1's
///    equality is not an observation about a re-executed row; it is a
///    restatement of the re-executor's own choice, and a guard whose input the
///    guarded party supplies is not a guard.
///
/// So it adds findings, and the findings may only move the verdict away from
/// `supported`, exactly as `with_finding` already guarantees.
fn fold_reexecutions(
    mut record: VerdictRecord,
    session: &SessionView,
    inputs: &Inputs,
    reexecutions: &[&CheckView],
    captured_binding_counted: Option<bool>,
) -> VerdictRecord {
    for r in reexecutions {
        record.evidence.push(r.id.clone());
        let Some(facts) = r.reexecution.as_ref() else {
            continue;
        };

        // **A re-execution is still ordered, and still read for narrowing.**
        // Not promoting it must not mean not examining it: a re-execution can
        // itself be superseded by a write no tree can see (a gitignored
        // `sitecustomize.py` that Python imports), and a re-executed
        // `pytest -k` is a narrowed run whoever ran it. Dropping either
        // detection in exchange for the promotion fix would trade one silence
        // for another.
        if let Binding::Reexecuted { underlying, .. } = bind(session, r) {
            match underlying.as_ref() {
                Binding::SupersededByEdit { .. } | Binding::SupersededByUnobservedChange { .. } => {
                    record = record.with_finding(Finding::new(
                        Severity::Fail,
                        "reexecution_superseded",
                        format!(
                            "the re-execution of `{}` is itself evidence for a tree that \
                             is not the final one — {}",
                            r.command(),
                            underlying.describe()
                        ),
                    ));
                }
                Binding::Unobservable { reason, .. } => {
                    record = record.with_finding(Finding::new(
                        Severity::Warn,
                        "reexecution_unbindable",
                        format!(
                            "the re-execution of `{}` could not be bound: {reason}",
                            r.command()
                        ),
                    ));
                }
                _ => {}
            }
        }
        if let Some(argv) = r.argv.as_deref() {
            if let Some(flag) = narrowing_flag(argv) {
                record = record.with_finding(Finding::new(
                    if inputs.scope_is_all {
                        Severity::Fail
                    } else {
                        Severity::Warn
                    },
                    "narrowed_run",
                    format!(
                        "the re-execution `{}` narrows the run with `{flag}`, so it \
                         exercised a subset of the suite",
                        r.command()
                    ),
                ));
            }
        }
        record = match facts.comparison.as_deref() {
            // §7: a mismatch is `unsupported`, with both records attached.
            Some("disagrees") => record.with_finding(Finding::new(
                Severity::Fail,
                "reexecution_contradicts",
                format!(
                    "a re-execution of `{}` in {} disagreed with the agent's own run",
                    r.command(),
                    facts.image_digest
                ),
            )),
            // **M3.3 — §6.3: "flaky tests never count as passing evidence."**
            //
            // `reexec` re-runs a failing command up to `policy.reexec.reruns`
            // times and records the LAST run, so a fail-then-pass flake is
            // stored as a check that exited 0 with a tally agreeing with the
            // agent's. Every other clause of this verifier is satisfied by it.
            // This one is the only thing standing between a suite that does
            // not produce one answer and a `supported` claim — and it is
            // stated deliberately, because the veto that used to cover the
            // case (`reexecution_failed`, above) covered it only for as long
            // as re-execution ran the command exactly once.
            Some("flaky") => record.with_finding(Finding::new(
                Severity::Fail,
                "reexecution_flaky",
                format!(
                    "re-executions of `{}` in {} did not agree with each other, so no run \
                     of it — the agent's included — is evidence that the suite passes",
                    r.command(),
                    facts.image_digest
                ),
            )),
            // Ran, and could not be compared. Not agreement — the same
            // three-way distinction `reexec`'s exit code 2 draws.
            Some("not_comparable") => record.with_finding(Finding::new(
                Severity::Warn,
                "reexecution_not_comparable",
                format!(
                    "a re-execution of `{}` ran and could not be compared with the \
                     agent's run, so it neither confirms nor contradicts it",
                    r.command()
                ),
            )),
            Some("agrees") => record.with_finding(Finding::new(
                Severity::Info,
                "reexecution_confirms",
                format!(
                    "a re-execution of `{}` in {} agreed with the agent's own run",
                    r.command(),
                    facts.image_digest
                ),
            )),
            // `--mode full`: the agent never ran this command, so there was
            // nothing to compare. Recorded as its own thing rather than as a
            // comparison that failed.
            None => record.with_finding(Finding::new(
                Severity::Warn,
                "reexecution_uncompared",
                format!(
                    "`{}` was re-executed under `--mode full`; the agent never ran it, \
                     so no comparison was made",
                    r.command()
                ),
            )),
            // **The seam, closed.** The four arms above are one spelling of the
            // outcome set; `reexec_cmd::record_comparison` is the other, and
            // nothing but a doc comment holds them together — see
            // `crates/cli/tests/m3_outcome_seam.rs`, which derives the writer's
            // set and fails when the two diverge.
            //
            // While this arm was `Severity::Warn` it moved no verdict, so a
            // comparison outcome this verifier could not read left the claim
            // `supported` — measured, with the string `"flakey"`. That is the
            // seam failing OPEN: the one direction §6 forbids, because a
            // comparison nobody can read is not agreement. `Fail` is the only
            // reading that cannot promote: an outcome from a newer writer says
            // *something* about the re-execution, and "I cannot read it" is not
            // evidence that the suite passes.
            Some(other) => record.with_finding(Finding::new(
                Severity::Fail,
                "reexecution_outcome_unrecognised",
                format!(
                    "a re-execution of `{}` recorded comparison outcome {other:?}, which \
                     this verifier does not know how to read — so it neither confirms the \
                     agent's run nor can be set aside, and the claim cannot rest on it. \
                     This verifier and `thruline reexec` are at different versions of the \
                     outcome set",
                    r.command()
                ),
            )),
        };
    }

    // **The sentence that used to be deleted.** When the session's own evidence
    // did not bind and a re-execution passed anyway, the reader has to be told
    // that a re-execution happened AND that it did not change the answer —
    // otherwise the next reader reruns it and believes the better number.
    if !reexecutions.is_empty() && captured_binding_counted == Some(false) {
        record = record.with_finding(Finding::new(
            Severity::Warn,
            "reexecution_did_not_promote",
            format!(
                "{} re-execution(s) ran at the final tree, and none of them promotes this \
                 verdict: the session's own test evidence does not bind to the tree the \
                 claim is about, and a re-execution confirms or contradicts a captured \
                 run rather than replacing one",
                reexecutions.len()
            ),
        ));
    }
    record
}

/// Raise a source-derived finding to `fail` when the *run* confirms it.
///
/// `skip_marker_added` was `fail` unconditionally, which accused every
/// deliberate `@pytest.mark.skipif(sys.platform == "win32")` of cheating. A
/// skip marker that changes no outcome is not evidence of anything; one that
/// leaves tests unrun is. The run says which, and the detector cannot.
fn escalate(finding: Finding, parsed: &ParsedView) -> Finding {
    if finding.detector != "skip_marker_added" {
        return finding;
    }
    // `skipped` only, deliberately: an `xfail` marker is already reported by
    // §6.3's own skips rule as a warning, and counting it here too would make a
    // deliberate `@pytest.mark.xfail(reason=…)` on a known-flaky test refute a
    // claim twice over.
    let unrun = parsed.skipped;
    if unrun == 0 {
        // A LOWERING, not a raise. A cap is a ceiling and never a floor, so
        // this path does not consult it — but it still goes through the type,
        // so that no site outside `verdict.rs` assigns a severity at all.
        let mut out = finding.lowered_to(Severity::Info);
        out.detail = format!(
            "{} — but the run reported no skipped tests, so the marker left \
             nothing unrun",
            out.detail
        );
        return out;
    }
    // R8: the raise goes through the value, which carries its pack's ceiling.
    // This site used to build `Finding { severity: Severity::Fail, ..finding }`
    // directly, which is how a capped Ruby `skip_marker_added` reached `fail`
    // and refuted a claim the operator had ruled ineligible to refute.
    let mut out = finding.raised_to(Severity::Fail);
    out.detail = format!(
        "{} — and the run left {unrun} test(s) unrun, so the marker took effect",
        out.detail
    );
    out
}

/// The first narrowing flag in an argv, if any.
///
/// Also catches a single named-file invocation, which narrows without a flag —
/// `pytest tests/test_one.py` runs one file and looks like a full run in every
/// other respect.
/// Where the RUNNER's own arguments begin.
///
/// # IR-6: `python3 -m pytest` is not a narrowed run
///
/// `-m` is in [`NARROWING_FLAGS`] because it is pytest's marker filter. In
/// `python3 -m pytest tests/` it is the **interpreter's** module flag, and
/// scanning from index 1 read it as pytest's — so the commonest way to invoke
/// pytest in a virtualenv reported `narrowed_run`, and an honest full suite
/// could not support a claim about the whole suite. Track D measured it against
/// the shipped binary while walking its own quickstart.
///
/// The flags in that list belong to the runner, so the scan starts where the
/// runner's argv does. Deliberately narrow: only the measured form
/// (`<python> -m <runner>`) is skipped past, because a wider rule would have to
/// guess which of `uv`, `npx`, `poetry` or `cargo` forward their tail and which
/// consume it, and guessing here refuses honest evidence in exactly the
/// direction §6.3 must not.
///
/// A real marker filter still counts: `python3 -m pytest -m slow` narrows,
/// because the scan begins at the runner and finds the second `-m`.
fn runner_args_start(argv: &[String]) -> usize {
    let is_python = argv.first().is_some_and(|a| {
        let f = a
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(a)
            .to_ascii_lowercase();
        f == "python" || f == "python3" || f.starts_with("python3.") || f == "py"
    });
    if is_python && argv.get(1).is_some_and(|a| a == "-m") && argv.len() > 2 {
        3
    } else {
        1
    }
}

pub fn narrowing_flag(argv: &[String]) -> Option<String> {
    let start = runner_args_start(argv);
    for arg in argv.iter().skip(start) {
        let lower = arg.to_ascii_lowercase();
        let head = lower.split('=').next().unwrap_or(&lower);
        if NARROWING_FLAGS.contains(&head) {
            return Some(head.to_string());
        }
    }
    // A bare argument only narrows if it actually names a path. `cargo test`'s
    // subcommand is the word "test", and reading it as a test path made every
    // full `cargo test` run look narrowed — the direction that refuses honest
    // evidence.
    let named_files: Vec<&String> = argv
        .iter()
        .skip(start)
        .filter(|a| {
            !a.starts_with('-')
                && (a.contains('/') || a.contains('\\') || a.contains('.'))
                && is_test_path(a)
        })
        .collect();
    if !named_files.is_empty() {
        return Some(format!("a named path ({})", named_files[0]));
    }
    None
}

/// Did the same command on the same tree produce different outcomes?
fn nondeterministic(checks: &[&CheckView]) -> Option<String> {
    for (i, a) in checks.iter().enumerate() {
        for b in checks.iter().skip(i + 1) {
            // Two checks whose commands are unrecorded are not thereby the
            // same command: `None == None` would have made every pair of them
            // comparable, and a flakiness verdict rests on the two runs being
            // the same run.
            let (Some(a_argv), Some(b_argv)) = (a.argv.as_deref(), b.argv.as_deref()) else {
                continue;
            };
            if a_argv != b_argv || a.input_tree_hash != b.input_tree_hash {
                continue;
            }
            let outcome = |c: &CheckView| (c.exit_code, c.parsed.as_ref().map(|p| p.failed));
            if outcome(a) != outcome(b) {
                return Some(format!(
                    "`{}` ran twice on the same tree with different outcomes; a flaky \
                     pass is not evidence",
                    a.command()
                ));
            }
        }
    }
    None
}
