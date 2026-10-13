//! **The ruling**, in one function, for the check run §9's App posts.
//!
//! Operator ruling, 2026-09-05, quoted in full because every branch below is
//! one of its clauses:
//!
//! > *with a policy the policy decides; without one, any unsupported claim
//! > yields neutral with the verdict table, never success; failure only from
//! > policy.*
//!
//! # The three clauses, and what each one is defending against
//!
//! **"with a policy the policy decides."** A repository that has written
//! `.thruline/policy.yml` has said which unproved claims may merge. The App
//! does not second-guess it — but it also cannot let it *lift*: a policy that
//! passes cannot green a receipt whose integrity failed, because the policy was
//! written about claims and knows nothing about whether the receipt is even
//! about this repository. So the policy decides **downward**, which is the same
//! rule `attest::report::Conclusion::worst` already enforces one layer in.
//!
//! **"without one, any unsupported claim yields neutral with the verdict table,
//! never success."** This is the measured defect, not a preference. At `10dae44`
//! a genuine receipt carrying `tests_pass: unsupported` produced
//! `conclusion: success` with a green badge. The receipt *was* genuine — the
//! integrity axis was right — and the sentence a reviewer read was "this change
//! is fine". The verdict table is part of the clause, not decoration: a neutral
//! with no table is "ThruLine looked and won't say", which is the second-worst
//! thing this surface can do.
//!
//! **"failure only from policy."** Deciding that an unproved claim may not merge
//! is the repository's call. This App reports; it does not sentence. The one
//! exception is not an exception to this clause at all: an integrity `failure`
//! means a check *contradicted* the receipt — the receipt is not about this
//! code, or it was altered — and that is not a claim going unproved, it is the
//! evidence not being evidence. §9 and the shipped Action have said so since
//! M2, and reading the clause as "an altered receipt is neutral" would make the
//! signature check unable to speak.
//!
//! # Why this is not `attest::report::conclusion_of`
//!
//! It mostly is, and deliberately: [`decide`] **carries** the conclusion the
//! core verifier printed rather than recomputing it, so the App and the CLI
//! cannot disagree about one receipt. What lives here is the part the core has
//! no opinion about, because it never runs without a receipt: **the
//! no-receipt case**, which is §9's `require_receipt`.

use thruline_policy::{Conclusion, Decision};

use crate::verify_json::VerifyJson;

/// Why the conclusion is what it is. One of these is always named in the check
/// run, so a reviewer never has to infer which axis spoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    /// Every claim proved, the receipt reproduces, and any policy passed.
    Verified,
    /// A check contradicted the receipt: it is not about this code, or it was
    /// altered. The integrity axis, and the one `failure` policy did not cause.
    ReceiptContradicted,
    /// The receipt is intact and carries **no claims at all**.
    ///
    /// Distinct from [`Ground::Verified`] because the sentence differs: nothing
    /// was proved here, there was simply nothing to prove. Both remain
    /// `success` — that is the operator's ruling and this does not touch it —
    /// but a zero-claim receipt used to render as *"Claims supported by the
    /// execution record"*, a statement about claims printed over a receipt with
    /// none, byte-identical to a genuinely proved one.
    ///
    /// Reachable two ways that this crate **cannot tell apart**: a session that
    /// honestly claimed nothing, and a claim extraction that produced nothing.
    /// The first deserves green; the second is a silent failure wearing it. The
    /// distinction needs the extractor recorded per claim, which is D22's
    /// `ClaimEntry::extractor`, so it is filed rather than guessed — and until
    /// then the title says what is true instead of implying the first.
    NoClaimsToCheck,

    /// Something in the receipt went unproved and no policy was present to
    /// judge it. **Neutral, with the table.**
    UnprovedNoPolicy,
    /// A policy was present and refused.
    PolicyRefused,
    /// A policy was present and could not decide.
    PolicyUndecided,
    /// Something went unproved and the policy allowed it. Neutral: the policy
    /// said it may merge, not that it was proved.
    UnprovedPolicyAllows,
    /// No receipt, and `require_receipt: true`.
    NoReceiptRequired,
    /// No receipt, and nothing required one.
    NoReceipt,
}

impl Ground {
    /// The one-line title, naming the axis that spoke.
    pub fn title(self) -> &'static str {
        match self {
            Ground::Verified => "Claims supported by the execution record",
            Ground::NoClaimsToCheck => {
                "Receipt intact; it carries no claims, so no claim was checked"
            }
            Ground::ReceiptContradicted => "Receipt NOT verified — a check contradicted it",
            Ground::UnprovedNoPolicy => {
                "Claim(s) NOT supported by the record — the receipt is genuine, the claim is not"
            }
            Ground::PolicyRefused => {
                "Policy FAILED — a claim this repository requires is not supported by the record"
            }
            Ground::PolicyUndecided => "Policy could not decide — not the same as passing",
            Ground::UnprovedPolicyAllows => {
                "Claim(s) unproved; the policy allows them to merge — not the same as proved"
            }
            Ground::NoReceiptRequired => "No receipt, and `require_receipt: true`",
            Ground::NoReceipt => "No ThruLine receipt on this pull request",
        }
    }
}

/// The check run's conclusion and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ruling {
    pub conclusion: Conclusion,
    pub ground: Ground,
    /// `true` when [`conclusion`] is the word the core verifier printed, rather
    /// than one this crate chose. False only where there was no receipt to run
    /// the verifier over.
    ///
    /// [`conclusion`]: Ruling::conclusion
    pub carried: bool,
}

/// **The ruling.**
///
/// `receipt` is [`crate::verify_json::parse`]'s output — already the core's own
/// conclusion over integrity *and* claims. `policy` is `.thruline/policy.yml`
/// evaluated by `thruline-policy`, or `None` when the repository has no policy
/// file. `require_receipt` comes from that same file and is `false` when there
/// is none: a repository that has not written a policy must not acquire one.
pub fn decide(
    receipt: Option<&VerifyJson>,
    policy: Option<&Decision>,
    require_receipt: bool,
) -> Ruling {
    let Some(r) = receipt else {
        // The core never runs here, so this is the one conclusion this crate
        // chooses, and it is chosen the same way as everything else: the
        // repository's policy decides whether an absent receipt blocks.
        return if require_receipt {
            Ruling {
                conclusion: Conclusion::Failure,
                ground: Ground::NoReceiptRequired,
                carried: false,
            }
        } else {
            Ruling {
                conclusion: Conclusion::Neutral,
                ground: Ground::NoReceipt,
                carried: false,
            }
        };
    };

    // The receipt's own two axes, as the core combined them. Never recomputed.
    let carried = r.conclusion;
    let unproved = r.claims.anything_unproved();
    // **A count that can go to zero needs a branch for zero.** `anything_unproved`
    // is three `> 0` comparisons, so a receipt with no claims answers "nothing
    // unproved" in exactly the words a fully proved one uses. Track G's arm of
    // tonight's defect: a check that runs today but could go empty. The green is
    // unchanged; only the sentence under it stops overclaiming.
    let verified = if r.claims.total == 0 {
        Ground::NoClaimsToCheck
    } else {
        Ground::Verified
    };
    let contradicted = carried == Conclusion::Failure;

    match (policy, contradicted, unproved) {
        // Integrity failure. Not a claim going unproved — the evidence is not
        // evidence. No policy can lift it, and none caused it.
        (_, true, _) => Ruling {
            conclusion: Conclusion::Failure,
            ground: Ground::ReceiptContradicted,
            carried: true,
        },

        // "failure only from policy": with no policy file, an unproved claim is
        // neutral. Never success — the clause names the exact word.
        (None, false, true) => Ruling {
            conclusion: Conclusion::Neutral,
            ground: Ground::UnprovedNoPolicy,
            carried: true,
        },
        (None, false, false) => Ruling {
            // Still carried: `neutral` is reachable with nothing unproved when
            // the checkout is shallow and the integrity axis could not run.
            conclusion: carried,
            ground: if carried == Conclusion::Success {
                verified
            } else {
                Ground::UnprovedNoPolicy
            },
            carried: true,
        },

        // "with a policy the policy decides" — downward only. `worst` is why a
        // passing policy cannot green an unproved claim: the policy said it may
        // merge, which is a different sentence from "it was proved".
        (Some(d), false, _) => {
            let combined = worst(carried, d.conclusion);
            let ground = match (d.conclusion, unproved) {
                (Conclusion::Failure, _) => Ground::PolicyRefused,
                (Conclusion::Neutral, _) => Ground::PolicyUndecided,
                (Conclusion::Success, true) => Ground::UnprovedPolicyAllows,
                (Conclusion::Success, false) => {
                    if combined == Conclusion::Success {
                        verified
                    } else {
                        Ground::UnprovedNoPolicy
                    }
                }
            };
            Ruling {
                conclusion: combined,
                ground,
                carried: true,
            }
        }
    }
}

/// Failure dominates neutral; neutral dominates success. **Downward only.**
pub fn worst(a: Conclusion, b: Conclusion) -> Conclusion {
    let rank = |c: Conclusion| match c {
        Conclusion::Success => 0u8,
        Conclusion::Neutral => 1,
        Conclusion::Failure => 2,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}
