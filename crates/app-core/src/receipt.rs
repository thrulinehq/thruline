//! Reading the fields `.thruline/policy.yml` needs out of a receipt.
//!
//! # Why the App builds the policy subject from the receipt
//!
//! `thruline policy` on a laptop reads the ledger, because the ledger is there.
//! A hosted gate has no ledger and never will — it has a DSSE envelope someone
//! pushed, and the pull request it arrived on. So the subject is assembled from
//! those two, and the difference is not cosmetic: some fields the CLI can
//! determine, this cannot, and **some fields only this can**.
//!
//! | field | on a laptop | here |
//! |---|---|---|
//! | claim verdicts | the ledger | the receipt's own `predicate.verdicts` |
//! | changed paths | `git diff` | the pull request's file list |
//! | `human_acked` | **never** — a label is not on a laptop | the PR's labels and reviews |
//! | changed-line coverage | the check's parsed output | the receipt, when it carried one |
//!
//! # `None` is not zero, and it is not false
//!
//! Every optional below stays `None` when the receipt does not carry the
//! field, and `thruline_policy` turns `None` into **neutral** under a rule that
//! needs it. Defaulting any of these to a passing value is the whole failure
//! this product exists against, one level down: a gate that assumes the
//! evidence it cannot see is not a gate.

use thruline_policy::{Claim, Subject, Verdict};

use crate::fetch::{base64_decode, FetchError};

/// The in-toto statement inside a DSSE envelope, as far as this crate reads it.
pub fn statement(envelope: &[u8]) -> Result<serde_json::Value, FetchError> {
    let env: serde_json::Value =
        serde_json::from_slice(envelope).map_err(|e| FetchError::NotJson(e.to_string()))?;
    let payload = env
        .get("payload")
        .and_then(|p| p.as_str())
        .ok_or(FetchError::Missing("payload"))?;
    let bytes = base64_decode(payload)?;
    serde_json::from_slice(&bytes).map_err(|e| FetchError::NotJson(e.to_string()))
}

/// The `sha256` of the DSSE envelope's bytes, which is what
/// [`crate::fetch::attestations`] looks a receipt up by, and what the commit
/// trailer records.
pub fn envelope_digest(envelope: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(envelope);
    h.finalize().iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn verdict(word: &str) -> Option<Verdict> {
    match word {
        "supported" => Some(Verdict::Supported),
        "unsupported" => Some(Verdict::Unsupported),
        "unverifiable" => Some(Verdict::Unverifiable),
        // A fourth word is **not mapped to a third**. It is dropped from the
        // policy subject and counted by the caller, because inventing a verdict
        // for a word this build does not know is how an unknown state becomes a
        // pass.
        _ => None,
    }
}

/// What the App could not read out of the receipt, so the check run can say so
/// rather than let a rule quietly evaluate against `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gaps {
    /// Verdict words this build does not recognise, named.
    pub unrecognised_verdicts: Vec<String>,
    /// Claims in the receipt with no verdict at all.
    pub claims_without_verdicts: usize,
}

/// Builds the policy subject.
///
/// `changed_paths` is the pull request's file list, or `None` when it could not
/// be fetched — and `None` makes `required_when` rules **apply** rather than be
/// skipped, which is `thruline_policy::applies`'s deliberate behaviour and the
/// reason this is passed through rather than defaulted to an empty list. An
/// empty `Vec` would switch every path-scoped rule off.
///
/// `human_acked` is `Some(true)` when the pull request carries the
/// acknowledgement §9's example names — a label or an approving review — and
/// `Some(false)` when the App looked and there was none. It is `None` only when
/// the App could not look.
pub fn subject(
    statement: &serde_json::Value,
    changed_paths: Option<Vec<String>>,
    human_acked: Option<bool>,
) -> (Subject, Gaps) {
    let mut gaps = Gaps::default();
    let p = statement.get("predicate");
    let verdicts = p
        .and_then(|p| p.get("verdicts"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let claim_kinds: std::collections::BTreeMap<String, (String, String)> = p
        .and_then(|p| p.get("claims"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let id = c.get("id")?.as_str()?.to_string();
                    let kind = c.get("kind")?.as_str()?.to_string();
                    // The extractor decides whether a claim may gate a merge at
                    // all (BUILD-SPEC §0.1). **Absent, it is advisory** — a
                    // receipt written before `ClaimEntry::extractor` existed
                    // says nothing about where its claims came from, and
                    // guessing `"rules"` would let a model-extracted claim fail
                    // someone's build. The cost is that an old receipt cannot
                    // gate, which the check run names rather than leaving a
                    // reader to wonder why a policy rule never fired.
                    let extractor = c
                        .get("extractor")
                        .and_then(|x| x.as_str())
                        .filter(|x| !x.is_empty())
                        .unwrap_or("unstated")
                        .to_string();
                    Some((id, (kind, extractor)))
                })
                .collect()
        })
        .unwrap_or_default();

    let checks: std::collections::BTreeMap<String, &serde_json::Value> = p
        .and_then(|p| p.get("checks"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| Some((c.get("id")?.as_str()?.to_string(), c)))
                .collect()
        })
        .unwrap_or_default();

    let mut claims = Vec::new();
    for v in &verdicts {
        let Some(claim_id) = v.get("claim_id").and_then(|x| x.as_str()) else {
            gaps.claims_without_verdicts += 1;
            continue;
        };
        let word = v.get("verdict").and_then(|x| x.as_str()).unwrap_or("");
        let Some(verdict) = verdict(word) else {
            gaps.unrecognised_verdicts.push(word.to_string());
            continue;
        };
        let (kind, extractor) = claim_kinds
            .get(claim_id)
            .cloned()
            .unwrap_or_else(|| (claim_id.to_string(), "unknown".to_string()));

        // Evidence is the verdict's own `evidence` list, and the check rows it
        // names are where `reexecuted`, coverage and skips come from.
        let evidence: Vec<&str> = v
            .get("evidence")
            .and_then(|e| e.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        let rows: Vec<&serde_json::Value> = evidence
            .iter()
            .filter_map(|id| checks.get(*id))
            .copied()
            .collect();

        claims.push(Claim {
            kind,
            extractor,
            verdict,
            // **Every one of these is `any`, not `all`**: a claim backed by two
            // checks of which one was re-executed *was* re-executed. `all`
            // would make adding evidence able to weaken a claim.
            reexecuted: rows
                .iter()
                .any(|c| c.get("reexecuted").and_then(|x| x.as_bool()) == Some(true)),
            evidence_present: !evidence.is_empty(),
            human_acked,
            changed_line_coverage: rows
                .iter()
                .filter_map(|c| {
                    c.get("coverage_changed_lines_pct")
                        .and_then(|x| x.as_f64())
                        .map(|f| f as u32)
                })
                .max(),
            // `None` and not `0`. A run whose skip count was never parsed has
            // an unknown number of skips, and `allow_skips: false` must be
            // neutral about it rather than read it as none.
            skipped_tests: rows
                .iter()
                .filter_map(|c| c.pointer("/parsed/skipped").and_then(|x| x.as_u64()))
                .max()
                .map(|n| n as u32),
        });
    }

    let promises = p.and_then(|p| p.get("promises"));
    let open_promises = promises
        .and_then(|x| x.get("open"))
        .and_then(|x| x.as_array())
        .map(|a| a.len() as u32)
        .unwrap_or(0);

    (
        Subject {
            receipt_present: true,
            changed_paths,
            open_promises,
            // **`stop_blocks` is deliberately not named here.** Track R's F11
            // was a literal `0` in this position making `max_stop_blocks`
            // unable to fire, and the argument that put a `0` here was the same
            // one: the App has no ledger, `stop_gate` is a session-time rule
            // (§4.1), a hosted gate is not at the session. Coherent, and wrong.
            // `Some(0)` **asserts** the gate blocked nothing, which this App
            // cannot know. "Could not be read" is its actual position, and it
            // makes `max_stop_blocks` conclude **neutral** on the hosted path
            // rather than be silently satisfied — narrowing the verdict, which
            // is the only direction this product moves.
            //
            // Omitting it and taking `Default` is not a dodge: the field is
            // `u32` at this branch's base and `Option<u32>` at the core's head,
            // and `Default` is `0` under the first and `None` under the second
            // — the right value under **both**, compiling under both. The
            // alternative was to write `None`, which does not compile here, and
            // hand the merge a line that must change as it lands. This is the
            // same line before and after, and it is correct at both ends.
            claims,
            // **`Default::default()` for this ONE field, and no `..` rest.**
            //
            // The value: `stop_blocks` is `u32` at this branch's base and
            // `Option<u32>` at the core's head, and `Default` is `0` under the
            // first and `None` under the second — right at both ends, compiling
            // at both, and the same line before and after the merge. Track R's
            // F11 was a literal `0` here making `max_stop_blocks` unable to
            // fire; `Some(0)` **asserts** the gate blocked nothing, which a
            // hosted App with no ledger cannot know, and `None` says it could
            // not be read, which makes that rule conclude **neutral** rather
            // than be silently satisfied.
            //
            // The shape: this was briefly `..Default::default()`, which is the
            // same value and a worse guard. A struct-update rest fills every
            // field nobody named, **including ones added later** — and `Subject`
            // gained a field today. That is F11's shape one level up: a gate
            // quietly given a value nobody computed. Naming every field means a
            // new one is a compile error in this file, which is where the
            // decision about what a hosted App can honestly claim belongs.
            stop_blocks: Default::default(),
        },
        gaps,
    )
}

/// The subject for a pull request with **no receipt**, so `require_receipt` can
/// fire from the policy rather than from a rule written twice.
pub fn absent_subject(changed_paths: Option<Vec<String>>) -> Subject {
    // **Every field named here too, for the reason the other site names them.**
    // This was `..Default::default()`, which produces the same values and
    // carries the property just removed from `subject()`: a field added to
    // `Subject` later would default silently in a policy subject, which is a
    // gate handed a value nobody computed. A guard on one of two construction
    // sites is not a guard, it is the appearance of one — and this is the site
    // for the case where there is *no receipt*, which is where a policy is most
    // likely to be the only thing standing.
    Subject {
        receipt_present: false,
        changed_paths,
        // **A known limitation, not a choice.** `open_promises` is `u32` on
        // both sides of the merge, so there is no way to say "no receipt, so
        // this could not be read" — and `0` means "none", which lets
        // `promises.block_on_open` conclude satisfied rather than neutral. Same
        // shape as F11, and unfixable from this crate: it needs the field to
        // become `Option<u32>` as `stop_blocks` did. Filed as R8.
        open_promises: 0,
        // `None` after the merge, `0` before — see `subject()` for why this is
        // the one field defaulted rather than written.
        stop_blocks: Default::default(),
        // No receipt means no claims to judge. This one is genuinely empty
        // rather than undetermined: the App looked and there was nothing.
        claims: Vec::new(),
    }
}
