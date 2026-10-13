//! The `thruline/receipt` check run payload (§9).
//!
//! # The conclusion is carried, never computed here
//!
//! §9's App verifies a receipt and evaluates a policy, and each of those
//! produces a conclusion. This module's job is to put them into the shape
//! GitHub's API wants — **not** to combine them by any rule of its own.
//! Combining is `combine`, it is four lines, and the four lines are the whole
//! argument: failure dominates, then neutral, and success requires that
//! everything actually succeeded.
//!
//! The measured reason this is written down: a `--json` consumer using
//! `all(c.outcome != "fail")` read a *partially verified* receipt as verified,
//! because such a receipt contains no failing check. Anywhere two conclusions
//! meet is a place that mistake can be made again.

use thruline_policy::{Conclusion, Decision};

use crate::receipt_source::Source;
use crate::ruling::Ruling;
use crate::verify_json::VerifyJson;

/// Failure dominates neutral; neutral dominates success.
///
/// `unverifiable` never becomes `supported`, one layer further out than §6 —
/// this is the same rule at the merge button, and it is the reason a green
/// check cannot be reached by two neutral halves.
pub fn combine(a: Conclusion, b: Conclusion) -> Conclusion {
    match (a, b) {
        (Conclusion::Failure, _) | (_, Conclusion::Failure) => Conclusion::Failure,
        (Conclusion::Neutral, _) | (_, Conclusion::Neutral) => Conclusion::Neutral,
        _ => Conclusion::Success,
    }
}

pub fn as_str(c: Conclusion) -> &'static str {
    match c {
        Conclusion::Success => "success",
        Conclusion::Failure => "failure",
        Conclusion::Neutral => "neutral",
    }
}

/// What GitHub's check-run API is sent.
///
/// `head_sha` is checked by [`crate::is_sha`] before it reaches here — the
/// caller cannot construct a `Subject` without that.
pub fn payload(
    head_sha: &str,
    conclusion: Conclusion,
    title: &str,
    summary: &str,
) -> serde_json::Value {
    serde_json::json!({
        "name": "thruline/receipt",
        "head_sha": head_sha,
        "status": "completed",
        "conclusion": as_str(conclusion),
        "output": {
            "title": title,
            "summary": summary,
        }
    })
}

/// The markdown body: what was checked, from where, and what it said.
pub fn summary(
    source: Source,
    receipt_conclusion: Conclusion,
    policy: Option<&Decision>,
) -> String {
    let mut out = String::new();
    let overall = match policy {
        Some(d) => combine(receipt_conclusion, d.conclusion),
        None => receipt_conclusion,
    };
    let headline = match overall {
        Conclusion::Success => {
            "✅ Every claim this pull request makes is supported by the \
                                execution record."
        }
        Conclusion::Failure => {
            "❌ A claim this pull request makes is contradicted by the \
                                execution record."
        }
        Conclusion::Neutral => {
            "◽ Nothing is blocked. Part of what was claimed could not be \
                                checked here."
        }
    };
    out.push_str(headline);
    out.push_str("\n\n");

    out.push_str(&format!(
        "**Receipt** — found via {}: `{}`.\n",
        source.as_str(),
        as_str(receipt_conclusion)
    ));
    // The caveat is never dropped. A receipt's signature makes its contents
    // trustworthy; nothing makes its presence meaningful except knowing who
    // could have put it there.
    if let Some(caveat) = source.caveat() {
        out.push_str(&format!("> {caveat}\n"));
    }
    out.push('\n');

    match policy {
        None => {
            out.push_str(
                // `--init`, not `init`. `thruline policy` takes no subcommand and
                // clap rejects the positional: `error: unexpected argument
                // 'init' found`. Verified by running it, not by reading main.rs.
                //
                // This string is the body of the check run posted on every pull
                // request, so the wrong form is the product telling every
                // reviewer to run a command that does not exist — and it had
                // already been corrected once, at `0331d72`, and regressed.
                // `crates/cli/tests/m6_docs_are_true.rs` is what catches it.
                "No `.thruline/policy.yml` in this repository, so nothing was gated. \
                 `thruline policy --init` writes the documented example.\n\n",
            );
        }
        Some(d) => {
            out.push_str(&format!("**Policy** — `{}`.\n\n", as_str(d.conclusion)));
            if d.reasons.is_empty() {
                out.push_str("No policy rule had anything to report.\n\n");
            } else {
                out.push_str("| | Claim | Rule | What the record shows |\n|---|---|---|---|\n");
                for r in &d.reasons {
                    let mark = match r.conclusion {
                        Conclusion::Failure => "❌",
                        Conclusion::Neutral => "◽",
                        Conclusion::Success => "✅",
                    };
                    let claim = r.claim.clone().unwrap_or_else(|| "—".to_string());
                    out.push_str(&format!(
                        "| {mark} | `{claim}` | `{}` | {} |\n",
                        r.rule,
                        r.detail.replace('|', "\\|")
                    ));
                }
                out.push('\n');
            }
            for (extractor, n) in &d.advisory {
                out.push_str(&format!(
                    "> {n} claim(s) from the `{extractor}` extractor are **advisory** and \
                     cannot fail this check. A model's output does not gate a merge \
                     (BUILD-SPEC §0.1).\n\n"
                ));
            }
        }
    }
    if overall == Conclusion::Neutral {
        out.push_str(
            "A neutral result is not a failure and not a pass. Most often the checkout \
             is shallow and the objects the receipt names are not present — \
             `fetch-depth: 0` on `actions/checkout` fixes that.\n",
        );
    }
    out
}

/// The one-line title.
pub fn title(conclusion: Conclusion) -> &'static str {
    match conclusion {
        Conclusion::Success => "Claims supported by the execution record",
        Conclusion::Failure => "A claim is contradicted by the execution record",
        Conclusion::Neutral => "Not everything could be checked",
    }
}

// ---------------------------------------------------------------------------
// What the App actually posts
// ---------------------------------------------------------------------------

/// The check run for one delivery, from [`crate::ruling::decide`]'s answer.
///
/// # Everything below is a rendering; nothing here decides
///
/// `ruling` arrives already decided, and in the ordinary case its `conclusion`
/// is the word `thruline verify-receipt --format json` printed — carried, not
/// recomputed. This function's only job is to put it in GitHub's shape and to
/// put the verdict table beside it, which is the half of the operator's clause
/// that a `conclusion` alone does not satisfy: *"neutral **with the verdict
/// table**"*.
///
/// The full comment body goes in **`summary`**, which is what `payload` emits.
///
/// This comment said `text` — GitHub's check-run API has such a field, rendered
/// under the summary, and `payload` has never populated it. The behaviour was
/// right and the description was wrong, so a reader checking whether the
/// verdict table reaches the pull request would have looked at the wrong field
/// and found it empty. Caught by Track Z reading this lane at `924c853` for
/// D22, not by anything here: no test asserts what a doc comment claims.
pub fn for_ruling(
    head_sha: &str,
    ruling: &Ruling,
    source: Option<Source>,
    receipt: Option<&VerifyJson>,
    policy: Option<&Decision>,
) -> serde_json::Value {
    let mut v = payload(
        head_sha,
        ruling.conclusion,
        ruling.ground.title(),
        &crate::comment::render(ruling, source, receipt, policy),
    );
    // The last guard before the wire, and it can only refuse. If anything above
    // has produced `success` beside an unproved claim, the payload is not
    // publishable — and the caller gets `null` rather than a corrected one,
    // because silently repairing a contradiction is how the contradiction
    // survives to the next reader.
    if ruling.conclusion == Conclusion::Success
        && receipt.is_some_and(|r| r.claims.anything_unproved())
    {
        return serde_json::Value::Null;
    }
    if let Some(map) = v.as_object_mut() {
        map.insert(
            "details_url".into(),
            serde_json::Value::String(
                "https://thrulinehq.com/spec/execution-check/v0.1".to_string(),
            ),
        );
    }
    v
}
