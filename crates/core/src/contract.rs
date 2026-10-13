//! Typed claim contracts, v0 — deterministic verification for marker claims.
//!
//! North Star v2 makes typed contracts the core mechanism and says plainly that
//! models "never fabricate the proof and are never the final judge". A33 brings
//! the first contract forward from M4 because the crown-jewel catch — *claimed
//! 33/33, the run recorded 31 passed and 2 failed* — is **arithmetic**, and
//! arithmetic has no false-positive rate to calibrate.
//!
//! NLI is demoted on these claims to explanation and tie-breaking. It is not in
//! the verdict path.
//!
//! ## What this deliberately refuses to call a contradiction
//!
//! The M3.9 brief specifies "mismatched counts → contradicted". This module
//! **does not implement that**, and the reason is measured rather than
//! theoretical: M3.8's hand-adjudication found *"306 tests green (303 + 3
//! new)"* and *"Full App suite green, 123 tests"* to be false alarms. A
//! developer's claim routinely aggregates several suite runs while the matched
//! evidence is only one of them, so a bare count mismatch is at least as likely
//! to mean "we matched the wrong run" as "the claim is false".
//!
//! What survives is the pair of comparisons that cannot be explained away by
//! aggregation:
//!
//! * an **all-pass** claim against a run that recorded failures, and
//! * a **success** claim against a run that recorded failure.
//!
//! Both are contradictions in the strict sense: the same family of evidence
//! says the opposite of the claim. A count mismatch with zero failures is
//! reported as `unsupported` — we could not confirm it, which is true, rather
//! than accusing on an ambiguity.

use crate::premise::Outcome;

/// The family of thing a claim or a piece of evidence is about. Matching is
/// only ever attempted within a family: a build result cannot contradict a
/// test-count claim, however alarming the scorer finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Test,
    Build,
    None,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Test => "test",
            Family::Build => "build",
            Family::None => "none",
        }
    }
    pub fn parse(s: &str) -> Family {
        match s {
            "test" => Family::Test,
            "build" => Family::Build,
            _ => Family::None,
        }
    }
}

/// The direction a claim affirmatively asserts.
///
/// §5q makes this three-valued on purpose. An open-world "anything not
/// recognised as failure must mean success" default is exactly how a new
/// mutation-testing dialect turned five truthful failure reports into
/// convictions in Round 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimPolarity {
    AssertsSuccess,
    AssertsFailure,
    Uncertain,
}

impl ClaimPolarity {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimPolarity::AssertsSuccess => "asserts-success",
            ClaimPolarity::AssertsFailure => "asserts-failure",
            ClaimPolarity::Uncertain => "uncertain",
        }
    }
}

/// What a marker claim asserts, in numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimAssertion {
    pub family: Family,
    /// A specific pass count the claim commits to, when it names one.
    pub passed: Option<u32>,
    /// A specific failure count the claim reports, when it names one.
    pub failed: Option<u32>,
    /// True when the claim asserts that *everything* passed / nothing failed.
    pub all_pass: bool,
    /// §5q: uncertain never convicts.
    pub polarity: ClaimPolarity,
}

/// What the recorded evidence says, in numbers. Derived once at annotate time
/// from the same `classify_outcome` the premise renderer uses, so the contract
/// and the prose can never disagree about what the run reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceOutcome {
    pub family: Family,
    pub passed: Option<u32>,
    pub failed: Option<u32>,
    /// `Some(false)` only when the run actually reported failure.
    pub ok: Option<bool>,
}

impl EvidenceOutcome {
    pub fn from_outcome(o: &Outcome) -> Self {
        match o {
            Outcome::Tests { passed, failed } => EvidenceOutcome {
                family: Family::Test,
                passed: *passed,
                failed: *failed,
                // Unknown failure count means unknown health. Defaulting to
                // "clean" here would let a partially-read run confirm an
                // all-pass claim (§5e).
                ok: failed.map(|f| f == 0),
            },
            Outcome::Succeeded => EvidenceOutcome {
                family: Family::Build,
                passed: None,
                failed: None,
                ok: Some(true),
            },
            Outcome::Failed => EvidenceOutcome {
                family: Family::Build,
                passed: None,
                failed: None,
                ok: Some(false),
            },
            Outcome::Unknown => EvidenceOutcome {
                family: Family::None,
                passed: None,
                failed: None,
                ok: None,
            },
        }
    }
}

/// The contract's verdict. Deliberately narrow: it either proves, disproves,
/// or declines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// The numbers agree.
    Supported(String),
    /// The same family of evidence says the opposite. An accusation.
    Contradicted(String),
    /// Same family, but the numbers cannot settle it (e.g. an aggregate count).
    Inconclusive(String),
    /// No contract applies — different family, or the claim names no numbers.
    NoContract,
}

/// Largest integer immediately preceding `kw` in `hay`, scanning every
/// occurrence. Mirrors the premise renderer's own parser so claim and evidence
/// are read by the same rules.
fn num_before(hay: &str, kw: &str) -> Option<u32> {
    let mut best: Option<u32> = None;
    let mut from = 0usize;
    while let Some(rel) = hay[from..].find(kw) {
        let idx = from + rel;
        let digits: String = hay[..idx]
            .chars()
            .rev()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            if let Ok(n) = digits.chars().rev().collect::<String>().parse::<u32>() {
                best = Some(best.map_or(n, |b: u32| b.max(n)));
            }
        }
        from = idx + kw.len();
    }
    best
}

/// `d+/d+` — "33/33", "104/104".
fn ratio(s: &str) -> Option<(u32, u32)> {
    let b: Vec<char> = s.chars().collect();
    for (i, c) in b.iter().enumerate() {
        if *c != '/' {
            continue;
        }
        let mut a = i;
        while a > 0 && b[a - 1].is_ascii_digit() {
            a -= 1;
        }
        let mut z = i + 1;
        while z < b.len() && b[z].is_ascii_digit() {
            z += 1;
        }
        if a < i && z > i + 1 {
            let l: String = b[a..i].iter().collect();
            let r: String = b[i + 1..z].iter().collect();
            if let (Ok(l), Ok(r)) = (l.parse(), r.parse()) {
                return Some((l, r));
            }
        }
    }
    None
}

/// Read a marker claim's numeric commitments. `None` when the claim commits to
/// nothing checkable — which is most claims, and is fine.
pub fn parse_claim(text: &str) -> Option<ClaimAssertion> {
    let low = text.replace(['*', '`', '_'], "").to_ascii_lowercase();

    let test_word = low.contains("test")
        || low.contains("spec")
        || low.contains("suite")
        || low.contains("pass")
        || low.contains("mutation")
        || low.contains("mutant");
    let build_word = low.contains("build")
        || low.contains("compile")
        || low.contains("clippy")
        || low.contains("lint")
        || low.contains("exit 0");
    let ratio_counts = ratio(&low);
    let labeled_passed = num_before(&low, " pass");
    let labeled_failed = num_before(&low, " fail")
        .or_else(|| num_before(&low, " failures"))
        .or_else(|| num_before(&low, " errors"))
        .or_else(|| num_before(&low, " tests are failing"))
        .or_else(|| num_before(&low, " tests failed"))
        .or_else(|| num_before(&low, " tests fail"));

    // §5o/§5q recall vocabulary, measured rather than imagined. The original
    // list covered "fails as expected"; Round 6 supplied mutation reports,
    // negative controls, deliberately poisoned code, and claims that simply
    // state the failing counts. These phrases recognise ASSERTS-FAILURE. They
    // do not relax any other conviction stage.
    let failure_word = low.contains("fail")
        || low.contains("is red")
        || low.contains("broke")
        || low.contains("killed")
        || low.contains("kills ");
    let intended_failure = low.contains("correctly")
        || low.contains("as expected")
        || low.contains("as intended")
        || low.contains("exactly as")
        || low.contains("deliberately")
        || low.contains("intentional")
        || low.contains("mutation check")
        || low.contains("mutation-check")
        || low.contains("mutant")
        || low.contains("negative control")
        || low.contains("poisoned")
        || low.contains("neutralized")
        || low.contains("neutralised")
        || low.contains("load-bearing")
        || low.contains("by design")
        || low.contains("fail-closed")
        || low.contains("fails closed")
        || low.contains("must fail")
        || low.contains("supposed to fail");

    // A named critic/probe/build that is explicitly meant to fail is one
    // recognised failure assertion even when the sentence also reports the
    // clean tree's passing suite. This is the measured §5o grammar
    // ("poisoned probe correctly fails; clean PurityGate 15/15") and preserves
    // its narrow support path without letting an unlabelled mixed narrative
    // choose a polarity.
    if failure_word
        && intended_failure
        && (low.contains("probe") || low.contains("compile") || low.contains("build"))
    {
        return Some(ClaimAssertion {
            family: Family::Build,
            passed: None,
            failed: None,
            all_pass: false,
            polarity: ClaimPolarity::AssertsFailure,
        });
    }

    let success_word = low.contains("succeed")
        || low.contains("clean")
        || low.contains(" green")
        || low.contains("tests pass")
        || low.contains("suite passes")
        || low.contains("0 errors")
        || low.contains("no errors")
        || low.contains("exit 0")
        || low.contains("all ")
        || low.contains("every ")
        || low.contains("0 failed")
        || low.contains("0 failures")
        || low.contains("no failures")
        || ratio_counts.is_some_and(|(p, total)| p == total);
    let explicitly_zero_failure = labeled_failed == Some(0)
        || low.contains("no failures")
        || low.contains("0 failed")
        || low.contains("0 failures");
    let named_failure = labeled_failed.is_some_and(|n| n > 0)
        || ratio_counts.is_some_and(|(p, total)| p < total)
        || (failure_word && !explicitly_zero_failure);

    let family = match (test_word || ratio_counts.is_some(), build_word) {
        (true, false) => Family::Test,
        (false, true) => Family::Build,
        // A mixed test+build narrative is not one typed claim. It remains
        // parseable so the receipt can name the abstention, but it has no
        // conviction polarity.
        (true, true) => Family::Test,
        (false, false) => return None,
    };
    let mixed_families = test_word && build_word;
    let polarity = if mixed_families || (success_word && named_failure && intended_failure) {
        ClaimPolarity::Uncertain
    } else if named_failure {
        ClaimPolarity::AssertsFailure
    } else if success_word && !named_failure {
        ClaimPolarity::AssertsSuccess
    } else {
        ClaimPolarity::Uncertain
    };

    let (passed, failed, all_pass) = if family == Family::Test {
        if let Some((p, total)) = ratio_counts {
            (Some(p), Some(total.saturating_sub(p)), p == total)
        } else {
            let passed = num_before(&low, " tests")
                .or_else(|| num_before(&low, " test"))
                .or_else(|| num_before(&low, " passing"))
                .or_else(|| num_before(&low, " passed"))
                .or(labeled_passed)
                .or_else(|| num_before(&low, " green"));
            let failed = labeled_failed.or_else(|| {
                (low.contains("no failures")
                    || low.contains("0 failed")
                    || low.contains("0 failures"))
                .then_some(0)
            });
            let all = polarity == ClaimPolarity::AssertsSuccess
                && (low.contains("all ")
                    || low.contains("every ")
                    || low.contains(" green")
                    || failed == Some(0)
                    || passed.is_some());
            (passed, failed, all)
        }
    } else {
        (None, None, false)
    };

    Some(ClaimAssertion {
        family,
        passed,
        failed,
        all_pass,
        polarity,
    })
}

/// The contract itself. Deterministic; no scores, no thresholds, no model.
pub fn check(claim: &ClaimAssertion, ev: &EvidenceOutcome) -> Check {
    // Families must match. A build result may not contradict a test claim.
    if claim.family != ev.family || ev.family == Family::None {
        return Check::NoContract;
    }

    if claim.polarity == ClaimPolarity::Uncertain {
        return Check::Inconclusive(
            "claim polarity is not affirmatively recognised; conviction is closed-world"
                .to_string(),
        );
    }

    // §5o: an asserts-failure claim inverts the comparison. A failing outcome
    // SUPPORTS it; a passing outcome is Inconclusive rather than a conviction,
    // because "the probe passes now" is at least as likely to mean we matched
    // a run from after the fix as it is to mean the claim lied.
    if claim.polarity == ClaimPolarity::AssertsFailure {
        let failed_as_intended = ev.ok == Some(false) || ev.failed.is_some_and(|f| f > 0);
        return if failed_as_intended {
            let exact_failed = claim.failed.zip(ev.failed).is_none_or(|(c, e)| c == e);
            let exact_passed = claim.passed.zip(ev.passed).is_none_or(|(c, e)| c == e);
            if exact_failed && exact_passed {
                Check::Supported(
                    "the recorded run failed, exactly as the claim reports".to_string(),
                )
            } else {
                Check::Inconclusive(
                    "the run failed, but its counts do not match the failure report".to_string(),
                )
            }
        } else {
            Check::Inconclusive(
                "the claim reports a failure and this run did not fail — \
                 possibly a different or later run"
                    .to_string(),
            )
        };
    }

    match claim.family {
        Family::Test => {
            let Some(failed) = ev.failed else {
                return Check::Inconclusive(
                    "the run did not report a failure count; outcome health is unknown".to_string(),
                );
            };
            // 1. The one comparison aggregation cannot explain away: the claim
            //    says nothing failed, and the recorded run says otherwise.
            if claim.all_pass && failed > 0 {
                return Check::Contradicted(format!(
                    "claimed all tests pass; the recorded run reported {} passed and {failed} failed",
                    ev.passed.unwrap_or(0)
                ));
            }
            match (claim.passed, ev.passed) {
                // 2. Exact agreement, nothing failed.
                (Some(c), Some(e)) if c == e && failed == 0 => {
                    Check::Supported(format!("the recorded run reported {e} passed and 0 failed"))
                }
                // 3. Counts differ but nothing failed. NOT a contradiction —
                //    see the module docs; a claim commonly totals several runs.
                (Some(c), Some(e)) if c != e => Check::Inconclusive(format!(
                    "claim names {c} tests; this run reported {e} passed and 0 failed \
                     — consistent with a partial or separate run, so not a conflict"
                )),
                // 4. Claim asserts all-pass, run recorded no failures.
                _ if failed == 0 => Check::Supported(format!(
                    "the recorded run reported {} passed and 0 failed",
                    ev.passed.unwrap_or(0)
                )),
                _ => Check::NoContract,
            }
        }
        Family::Build => match ev.ok {
            Some(false) if claim.polarity == ClaimPolarity::AssertsSuccess => Check::Contradicted(
                "claimed the build/lint succeeded; the recorded run reported failure".to_string(),
            ),
            Some(true) if claim.polarity == ClaimPolarity::AssertsSuccess => {
                Check::Supported("the recorded run completed successfully".to_string())
            }
            _ => Check::NoContract,
        },
        Family::None => Check::NoContract,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tests_ev(p: u32, f: u32) -> EvidenceOutcome {
        EvidenceOutcome::from_outcome(&Outcome::Tests {
            passed: Some(p),
            failed: Some(f),
        })
    }

    #[test]
    fn the_crown_jewel_is_arithmetic_not_inference() {
        // "claimed 33/33, the run recorded 31 passed / 2 failed"
        let c = parse_claim("33/33 tests pass.").expect("claim parses");
        assert!(matches!(
            check(&c, &tests_ev(31, 2)),
            Check::Contradicted(_)
        ));

        for t in [
            "48/48 pytest tests pass.",
            "All 214 tests pass.",
            "306 tests green (303 + 3 new).",
        ] {
            let c = parse_claim(t).unwrap_or_else(|| panic!("should parse: {t}"));
            assert!(
                matches!(check(&c, &tests_ev(45, 3)), Check::Contradicted(_)),
                "a run with failures must contradict an all-pass claim: {t:?}"
            );
        }
    }

    #[test]
    fn a_failed_build_contradicts_a_success_claim() {
        let c = parse_claim("Build succeeded, clippy clean.").expect("parses");
        assert_eq!(c.family, Family::Build);
        assert!(matches!(
            check(&c, &EvidenceOutcome::from_outcome(&Outcome::Failed)),
            Check::Contradicted(_)
        ));
        assert!(matches!(
            check(&c, &EvidenceOutcome::from_outcome(&Outcome::Succeeded)),
            Check::Supported(_)
        ));
    }

    #[test]
    fn a_count_mismatch_with_no_failures_is_never_an_accusation() {
        // The measured M3.8 false alarms. A developer's total legitimately
        // aggregates runs; the matched evidence is one of them.
        let c = parse_claim("306 tests green (303 + 3 new).").expect("parses");
        assert!(
            matches!(check(&c, &tests_ev(43, 0)), Check::Inconclusive(_)),
            "aggregate counts must not accuse"
        );
        let c = parse_claim("Full App suite green, 123 tests.").expect("parses");
        assert!(matches!(
            check(&c, &tests_ev(40, 0)),
            Check::Inconclusive(_)
        ));
    }

    #[test]
    fn families_never_cross() {
        let c = parse_claim("All 12 tests pass.").expect("parses");
        assert_eq!(
            check(&c, &EvidenceOutcome::from_outcome(&Outcome::Failed)),
            Check::NoContract,
            "a failed BUILD must not contradict a TEST claim"
        );
    }

    #[test]
    fn exact_agreement_is_support() {
        let c = parse_claim("All 12 tests pass.").expect("parses");
        assert!(matches!(check(&c, &tests_ev(12, 0)), Check::Supported(_)));
    }

    #[test]
    fn a_claim_about_failure_is_affirmatively_failure_polarity() {
        // Round 6: a claim REPORTING failure was convicted by the failure it
        // reported. Closed-world polarity recognises the direction instead of
        // treating "not success" as an untyped success claim.
        let tests = parse_claim("5 tests pass; 2 tests are failing.").expect("typed failure");
        assert_eq!(tests.polarity, ClaimPolarity::AssertsFailure);
        assert!(matches!(
            check(&tests, &tests_ev(5, 2)),
            Check::Supported(_)
        ));
        assert!(matches!(
            check(&tests, &tests_ev(7, 0)),
            Check::Inconclusive(_)
        ));

        let build = parse_claim("The build failed.").expect("typed build failure");
        assert_eq!(build.polarity, ClaimPolarity::AssertsFailure);
        assert!(matches!(
            check(&build, &EvidenceOutcome::from_outcome(&Outcome::Failed)),
            Check::Supported(_)
        ));
    }

    #[test]
    fn mixed_mutation_narratives_have_uncertain_polarity_and_never_convict() {
        let c = parse_claim(
            "Full suite 41/41 green; the intentional mutation check kills the poisoned branch.",
        )
        .expect("family recognised");
        assert_eq!(c.polarity, ClaimPolarity::Uncertain);
        assert!(matches!(
            check(&c, &tests_ev(39, 2)),
            Check::Inconclusive(_)
        ));
    }

    #[test]
    fn prose_without_numbers_has_no_contract() {
        assert!(parse_claim("I've added retry logic to the webhook client.").is_none());
        assert!(parse_claim("The session-identity rule was wrong.").is_none());
    }
}

// ---------------------------------------------------------------------------
// R17a-2 — the attested_run contract.
// ---------------------------------------------------------------------------

/// What an accepted, assured capture pair actually attests.
///
/// RULING 1, verbatim: the proposition is `exit_code` at `head_commit` under a
/// repository fingerprint that did not move across the run, for the command
/// bound by `command_sha256`. These are **capsule fields**. Nothing here reads
/// transcript text, and nothing here may be reconstructed from it — which is
/// the whole reason this contract exists separately from [`check`], whose
/// inputs come from `premise::classify_outcome` scanning prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestedRun {
    /// From `capture_attempts.strength`.
    pub assured: bool,
    /// From `capture_attempts.status`.
    pub accepted: bool,
    /// From `capture_attempts.command_sha256`.
    pub command_sha256: String,
    /// From `capture_attempts.exit_code`.
    pub exit_code: Option<i64>,
    /// From `state_capsules(phase='post').head_commit`.
    pub head_commit: String,
    /// `pre.state == post.state` — the repository fingerprint did not move
    /// across the run. `accept_capture_pair` already refuses the pair
    /// otherwise; carried explicitly so this contract states its own
    /// precondition rather than trusting a distant caller.
    pub fingerprint_stable: bool,
}

/// The verdict of the attested_run contract.
///
/// **Verified or abstain. There is deliberately no contradicted branch.** A
/// non-zero exit against a claim of success is a *disagreement between an
/// attested run and a sentence*, and turning that into a conviction is a
/// separate ruling with its own evidence bar — not a side effect of building a
/// producer. Until that ruling exists, the honest answer is that this contract
/// cannot confirm the claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttestedCheck {
    Verified(String),
    Abstain(String),
}

/// Evaluate the attested_run contract.
///
/// `claim_command_sha256` is the command identity the CLAIM's evidence is bound
/// to. It must equal the attested run's, or the capsule attests a different
/// command than the one the claim is about.
pub fn check_attested_run(
    claim: &ClaimAssertion,
    claim_command_sha256: Option<&str>,
    run: &AttestedRun,
) -> AttestedCheck {
    if !run.assured || !run.accepted {
        return AttestedCheck::Abstain(
            "the capture pair is not an accepted assured pair; a bounded observation \
             attests nothing about project state"
                .to_string(),
        );
    }
    if !run.fingerprint_stable {
        return AttestedCheck::Abstain(
            "the repository fingerprint moved across the run, so the exit code does not \
             pin the state the claim is about"
                .to_string(),
        );
    }
    match claim_command_sha256 {
        Some(sha) if sha == run.command_sha256 => {}
        Some(_) => {
            return AttestedCheck::Abstain(
                "the attested command is not the command this claim is about".to_string(),
            )
        }
        None => {
            return AttestedCheck::Abstain(
                "the claim's evidence carries no command identity to match against the \
                 attested run"
                    .to_string(),
            )
        }
    }
    // A capsule attests an exit code. It does NOT attest counts — no stdout or
    // stderr digest is recorded — so a claim that commits to numbers is making
    // a proposition this receipt cannot reach.
    if claim.passed.is_some() || claim.failed.is_some() {
        return AttestedCheck::Abstain(
            "the claim commits to counts, and an attested run carries an exit code, not \
             counts"
                .to_string(),
        );
    }
    if claim.polarity != ClaimPolarity::AssertsSuccess {
        return AttestedCheck::Abstain(
            "only an affirmative success claim is in this contract's scope".to_string(),
        );
    }
    match run.exit_code {
        Some(0) => AttestedCheck::Verified(format!(
            "the attested run of this exact command exited 0 at commit {}, and the \
             repository fingerprint did not move across it",
            &run.head_commit[..run.head_commit.len().min(12)]
        )),
        Some(_) | None => AttestedCheck::Abstain(
            "the attested run did not exit 0; this contract confirms or abstains, and a \
             conviction from an attested run is a separate ruling"
                .to_string(),
        ),
    }
}
