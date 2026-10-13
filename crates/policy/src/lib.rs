//! BUILD-SPEC §9's policy engine: `.thruline/policy.yml`, evaluated.
//!
//! # What this crate may and may not see
//!
//! **[BUILD-PLAN C.8.3]** Policy evaluates **rules-extracted claims only**. A
//! claim a model added is advisory: it appears in Explain, it may read
//! `unsupported`, and it can never fail a gate. The rationale is §0.1 — a
//! model-only claim that can block a merge has put the model into enforcement —
//! and this repository's own measurement backs it: `speaker_gate.rs` records 27
//! of 30 model-sourced emitted lines as false accusations.
//!
//! The filter is therefore one-directional by construction. `extractor` is read
//! here **only to restrict** what policy may act on, and the schema is closed
//! so that no policy key can reference it, a score, or a confidence — a test
//! asserts that, because a future field that reopened the door would look like
//! a feature.
//!
//! # This parses a documented subset, not YAML, and says so
//!
//! A policy file decides whether a merge is blocked, so the parser's failure
//! mode matters more than its coverage. Full YAML brings aliases, merge keys,
//! implicit typing (`no` → `false`, the Norway problem) and unbounded expansion
//! — every one of which turns "what the file says" into "what the parser
//! thought". So this reads a **closed schema**: a fixed key set, two-space
//! indentation, scalars, and inline flow lists. Anything else is an error
//! naming the line, never a shrug.
//!
//! That closure is the security property, not a shortcut. An unknown key is
//! refused rather than ignored, so a policy written against a newer ThruLine
//! fails loudly on an older one instead of silently enforcing less than its
//! author wrote.

use std::collections::BTreeMap;

pub mod acceptance;
pub mod decisions;
mod digest;
mod parse;
#[cfg(test)]
mod tests;

pub use digest::{absent_digest, digest_for_repo, policy_path, DigestError};
pub use parse::{Node, ParseError, SCHEMA};

/// The §9 policy document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub version: u32,
    pub require_receipt: bool,
    pub claims: BTreeMap<String, ClaimRule>,
    pub promises: Promises,
    pub stop_gate: StopGate,
    pub reexec: Reexec,
    /// `None` preserves the pre-Decisions policy behavior for existing files.
    pub decisions: Option<decisions::DecisionsPolicy>,
    /// Named proposals for a Change's done criteria. They do not affect the
    /// gate's verdict; the person still chooses and accepts the lines.
    pub templates: BTreeMap<String, DoneTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoneTemplate {
    pub lines: Vec<String>,
}

/// What policy demands of one claim kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimRule {
    /// The rule applies only when the PR touches one of these globs. `None`
    /// means always.
    pub required_when_paths: Option<Vec<String>>,
    pub require_reexecuted: bool,
    pub require_evidence: bool,
    pub require_human_ack: bool,
    pub min_changed_line_coverage: Option<u32>,
    /// `false` means a skipped test is a policy failure.
    pub allow_skips: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Promises {
    pub block_on_open: bool,
    pub comment_open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopGate {
    pub enabled: bool,
    pub max_stop_blocks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reexec {
    pub mode: ReexecMode,
    pub reruns: u32,
    pub image: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReexecMode {
    Claimed,
    Full,
}

impl Default for Policy {
    /// §9's shipped example, which is also what a repository with no policy
    /// file gets. **`require_receipt` defaults to false**: a gate that fails a
    /// repository which never opted in would teach people to remove it.
    fn default() -> Policy {
        Policy {
            version: 1,
            require_receipt: false,
            claims: BTreeMap::new(),
            promises: Promises {
                block_on_open: false,
                comment_open: true,
            },
            stop_gate: StopGate {
                enabled: false,
                max_stop_blocks: 2,
            },
            reexec: Reexec {
                mode: ReexecMode::Claimed,
                reruns: 2,
                image: "auto".to_string(),
            },
            decisions: None,
            templates: BTreeMap::new(),
        }
    }
}

/// The verdicts policy can see. Deliberately the §6 three, with no fourth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Supported,
    Unsupported,
    Unverifiable,
}

/// One claim, as policy sees it.
///
/// A separate type from the verifier's own, so this crate depends on nothing
/// and the dependency can never run the other way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub kind: String,
    /// `"rules"` or anything else. Anything else is advisory and cannot fail a
    /// gate — see the module docs.
    pub extractor: String,
    pub verdict: Verdict,
    pub reexecuted: bool,
    pub evidence_present: bool,
    /// `None` means **not determined here** — a PR label and an approving
    /// review do not exist on a developer's laptop. Under `require_human_ack`
    /// that is neutral, never a pass: a gate that assumes the acknowledgement
    /// it cannot see is not a gate.
    pub human_acked: Option<bool>,
    pub changed_line_coverage: Option<u32>,
    /// `None` means the run's skip count was not parsed, which is not the same
    /// as zero and must not read as zero.
    pub skipped_tests: Option<u32>,
}

/// What the run under policy looked like.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subject {
    pub receipt_present: bool,
    /// Repository-relative paths the change touched. **`None` means not
    /// determined**, and a `required_when` rule then applies rather than being
    /// silently skipped — an undetermined path set must not be able to switch a
    /// rule off.
    ///
    /// **`Some(vec![])` is undetermined too, and that is Track R's F9.** A diff
    /// that computed cleanly and named nothing is not the same observation as a
    /// change that touched nothing: an agent which commits before the session's
    /// last captured event leaves the head tree and the final snapshot tree as
    /// the same git object, and `git diff-tree` between them is empty. Read as
    /// "nothing changed", that empty list makes every path-scoped rule in the
    /// file unable to match — a policy silently switched off by the one
    /// circumstance in which nobody could see the change set.
    pub changed_paths: Option<Vec<String>>,
    pub open_promises: u32,
    /// How many times the §4.1 Stop gate blocked this session. **`None` means
    /// the block ledger could not be read**, which is not a count of zero: a
    /// literal `0` here is what made `max_stop_blocks` unable to fire for any
    /// input (Track R's F11).
    pub stop_blocks: Option<u32>,
    pub claims: Vec<Claim>,
}

/// A check run's conclusion.
///
/// `Neutral` is load-bearing: §9 says `unverifiable` produces a neutral
/// conclusion with instructions, **not** a failure and not a pass. It is §6's
/// three-state verdict surviving all the way to the merge button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conclusion {
    Success,
    Failure,
    Neutral,
}

/// Why the check concluded what it did. One per finding, in evaluation order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    pub claim: Option<String>,
    pub rule: &'static str,
    pub detail: String,
    pub conclusion: Conclusion,
}

/// The evaluation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub conclusion: Conclusion,
    pub reasons: Vec<Reason>,
    /// Claims policy was not allowed to act on, by extractor. **Reported, never
    /// hidden**: a reader must be able to see that a model-sourced
    /// `unsupported` existed and was advisory, rather than wonder why it did
    /// not appear.
    pub advisory: BTreeMap<String, usize>,
}

impl Policy {
    /// Evaluates the subject. Deterministic and total: same inputs, same
    /// decision, no I/O.
    pub fn evaluate(&self, subject: &Subject) -> Decision {
        let mut reasons: Vec<Reason> = Vec::new();
        let mut advisory: BTreeMap<String, usize> = BTreeMap::new();

        if self.require_receipt && !subject.receipt_present {
            reasons.push(Reason {
                claim: None,
                rule: "require_receipt",
                detail: "policy requires a receipt and none was found".to_string(),
                conclusion: Conclusion::Failure,
            });
        }

        for claim in &subject.claims {
            // **The one-directional filter.** Counted and reported, never
            // silently dropped.
            if claim.extractor != "rules" {
                *advisory.entry(claim.extractor.clone()).or_insert(0) += 1;
                continue;
            }
            let Some(rule) = self.claims.get(&claim.kind) else {
                continue;
            };
            match scope_of(rule, subject.changed_paths.as_ref()) {
                // **Skipped, and SAID.** (Track Q, D10.)
                //
                // This used to be a bare `continue`. The documented example
                // policy scopes `tests_pass` to `src/**` and `lib/**`; the
                // demo repository this product ships edits `shipping.py` and
                // `test_shipping.py` at the repository root, so the rule never
                // applied — and a session whose only claim was an
                // **unsupported** `tests_pass` passed the gate with nothing
                // anywhere saying which rule had stood down.
                //
                // Path scoping is defensible; scoping that is silent is not.
                // The conclusion is `Neutral` rather than `Failure` because a
                // rule that does not apply has not been broken — but it is a
                // reason, so it reaches the report, the check run and the
                // `--json` output, and a reader can see the gate declining to
                // look. `Conclusion::worst` treats a neutral beside a success
                // as neutral, which is the ruled ceiling and the safe
                // direction.
                Scope::Excluded => {
                    reasons.push(Reason {
                        claim: Some(claim.kind.clone()),
                        rule: "required_when",
                        detail: format!(
                            "policy scopes this rule to {} and this change touched none of them, so \
                             the rule did not apply. The claim's own verdict was `{}` \
                             and this gate did not judge it.",
                            rule.required_when_paths
                                .as_deref()
                                .unwrap_or_default()
                                .join(", "),
                            match claim.verdict {
                                Verdict::Supported => "supported",
                                Verdict::Unsupported => "unsupported",
                                Verdict::Unverifiable => "unverifiable",
                            }
                        ),
                        // **Neutral only when there was something to judge.**
                        // A scoped-out rule over a claim the record already
                        // SUPPORTS costs a reader nothing, and making every
                        // docs-only pull request amber would teach people to
                        // ignore amber. The fact is reported either way; the
                        // conclusion moves only when the gate declined to look
                        // at a claim that was not proven.
                        conclusion: match claim.verdict {
                            Verdict::Supported => Conclusion::Success,
                            Verdict::Unsupported | Verdict::Unverifiable => Conclusion::Neutral,
                        },
                    });
                    continue;
                }
                Scope::Applies => {}
                // The scope could not be checked. Evaluate the rule — a gate
                // that switches itself off when it cannot see the diff switches
                // itself off exactly when something has gone wrong — and say
                // so, because "this rule applied" and "this rule was in scope"
                // are different claims and only the first is true here.
                Scope::Undetermined => reasons.push(Reason {
                    claim: Some(claim.kind.clone()),
                    rule: "required_when",
                    detail: format!(
                        "policy scopes this rule to {} and the change set could not be \
                         determined, so the rule was evaluated rather than skipped",
                        rule.required_when_paths
                            .as_deref()
                            .unwrap_or_default()
                            .join(", ")
                    ),
                    conclusion: Conclusion::Neutral,
                }),
            }
            reasons.extend(evaluate_claim(&claim.kind, rule, claim));
        }

        if self.promises.block_on_open && subject.open_promises > 0 {
            reasons.push(Reason {
                claim: None,
                rule: "promises.block_on_open",
                detail: format!("{} open promise(s)", subject.open_promises),
                conclusion: Conclusion::Failure,
            });
        }
        if self.stop_gate.enabled {
            match subject.stop_blocks {
                Some(n) if n > self.stop_gate.max_stop_blocks => reasons.push(Reason {
                    claim: None,
                    rule: "stop_gate.max_stop_blocks",
                    detail: format!(
                        "{n} stop block(s), over the limit of {}",
                        self.stop_gate.max_stop_blocks
                    ),
                    conclusion: Conclusion::Failure,
                }),
                Some(_) => {}
                // **An unread ledger is not a ledger of zero.** Reporting the
                // limit as satisfied is what made this rule dead on arrival.
                None => reasons.push(Reason {
                    claim: None,
                    rule: "stop_gate.max_stop_blocks",
                    detail: format!(
                        "policy allows at most {} stop block(s) and this run cannot read the \
                         gate's block ledger — install the Stop gate (`thruline init`) or set \
                         `stop_gate.enabled: false`",
                        self.stop_gate.max_stop_blocks
                    ),
                    conclusion: Conclusion::Neutral,
                }),
            }
        }

        // Failure dominates neutral, and neutral dominates success. An
        // `unverifiable` claim must never be able to produce a green check —
        // that is §6's "`unverifiable` never collapses into `supported`" at the
        // merge button.
        let conclusion = if reasons.iter().any(|r| r.conclusion == Conclusion::Failure) {
            Conclusion::Failure
        } else if reasons.iter().any(|r| r.conclusion == Conclusion::Neutral) {
            Conclusion::Neutral
        } else {
            Conclusion::Success
        };
        Decision {
            conclusion,
            reasons,
            advisory,
        }
    }
}

/// Whether a rule is in scope for the paths the change touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The rule is unconditional, or a glob matched.
    Applies,
    /// The paths are known and none of them matched.
    Excluded,
    /// The path set is not knowable from here — absent, or computed and empty.
    Undetermined,
}

/// Does this rule apply to the paths the change touched?
fn scope_of(rule: &ClaimRule, changed: Option<&Vec<String>>) -> Scope {
    match (&rule.required_when_paths, changed) {
        (None, _) => Scope::Applies,
        // Not determined. Both forms of it: the diff was unavailable, or it ran
        // and named nothing — which happens whenever the session's head tree
        // and its final snapshot are the same object, and is indistinguishable
        // from a change that genuinely touched no file.
        (Some(_), None) => Scope::Undetermined,
        (Some(_), Some(paths)) if paths.is_empty() => Scope::Undetermined,
        (Some(globs), Some(paths)) => {
            if paths.iter().any(|p| globs.iter().any(|g| glob(g, p))) {
                Scope::Applies
            } else {
                Scope::Excluded
            }
        }
    }
}

/// `*` matches within a path segment; `**` crosses segments. Nothing else is a
/// metacharacter, because a policy glob that needed a regex engine would be a
/// policy nobody could read.
pub fn glob(pattern: &str, path: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        if p.is_empty() {
            return s.is_empty();
        }
        if p[0] == b'*' {
            if p.len() > 1 && p[1] == b'*' {
                // `**` — any run, separators included.
                let rest = if p.len() > 2 && p[2] == b'/' {
                    &p[3..]
                } else {
                    &p[2..]
                };
                for i in 0..=s.len() {
                    if go(rest, &s[i..]) {
                        return true;
                    }
                }
                return false;
            }
            // `*` — any run within one segment.
            for i in 0..=s.len() {
                if s[..i].contains(&b'/') {
                    break;
                }
                if go(&p[1..], &s[i..]) {
                    return true;
                }
            }
            return false;
        }
        !s.is_empty() && p[0] == s[0] && go(&p[1..], &s[1..])
    }
    go(pattern.as_bytes(), path.as_bytes())
}

fn evaluate_claim(kind: &str, rule: &ClaimRule, claim: &Claim) -> Vec<Reason> {
    let mut out = Vec::new();
    match claim.verdict {
        Verdict::Unsupported => out.push(Reason {
            claim: Some(kind.to_string()),
            rule: "verdict",
            detail: "the claim is unsupported by the execution record".to_string(),
            conclusion: Conclusion::Failure,
        }),
        Verdict::Unverifiable => out.push(Reason {
            claim: Some(kind.to_string()),
            rule: "verdict",
            detail: "the record does not contain enough to decide; run \
                     `thruline reexec` or capture the check under `thruline run`"
                .to_string(),
            conclusion: Conclusion::Neutral,
        }),
        Verdict::Supported => {}
    }
    if rule.require_reexecuted && !claim.reexecuted {
        out.push(Reason {
            claim: Some(kind.to_string()),
            rule: "require_reexecuted",
            detail: "policy requires the check to have been re-executed and it was not".to_string(),
            conclusion: Conclusion::Failure,
        });
    }
    if rule.require_evidence && !claim.evidence_present {
        out.push(Reason {
            claim: Some(kind.to_string()),
            rule: "require_evidence",
            detail: "policy requires evidence for this claim and none is bound to it".to_string(),
            conclusion: Conclusion::Failure,
        });
    }
    if rule.require_human_ack {
        match claim.human_acked {
            Some(true) => {}
            Some(false) => out.push(Reason {
                claim: Some(kind.to_string()),
                rule: "require_human_ack",
                detail: "policy requires a human acknowledgement (label or approving review) \
                         and none is present"
                    .to_string(),
                conclusion: Conclusion::Failure,
            }),
            None => out.push(Reason {
                claim: Some(kind.to_string()),
                rule: "require_human_ack",
                detail: "policy requires a human acknowledgement and this run cannot see \
                         one — labels and reviews exist on the pull request, not here"
                    .to_string(),
                conclusion: Conclusion::Neutral,
            }),
        }
    }
    if !rule.allow_skips {
        match claim.skipped_tests {
            Some(0) => {}
            Some(n) => out.push(Reason {
                claim: Some(kind.to_string()),
                rule: "allow_skips",
                detail: format!("{n} skipped test(s) and policy allows none"),
                conclusion: Conclusion::Failure,
            }),
            None => out.push(Reason {
                claim: Some(kind.to_string()),
                rule: "allow_skips",
                detail: "policy allows no skipped tests and the run's skip count was not \
                         parsed — an unparsed count is not a count of zero"
                    .to_string(),
                conclusion: Conclusion::Neutral,
            }),
        }
    }
    if let Some(min) = rule.min_changed_line_coverage {
        match claim.changed_line_coverage {
            Some(have) if have < min => out.push(Reason {
                claim: Some(kind.to_string()),
                rule: "min_changed_line_coverage",
                detail: format!("changed-line coverage {have}% is under the required {min}%"),
                conclusion: Conclusion::Failure,
            }),
            // **Absent coverage is neutral, not a pass.** A policy that asks
            // for 70% and gets no measurement has not been satisfied; treating
            // silence as success is the collapse §6 forbids.
            None => out.push(Reason {
                claim: Some(kind.to_string()),
                rule: "min_changed_line_coverage",
                detail: format!(
                    "policy requires {min}% changed-line coverage and none was measured"
                ),
                conclusion: Conclusion::Neutral,
            }),
            Some(_) => {}
        }
    }
    out
}
