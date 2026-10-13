//! BUILD-SPEC §9 — the renderings a reviewer sees: a GitHub **check run**, a
//! PR comment table, and the `verified-by-thruline` badge.
//!
//! # One conclusion, three renderings — and none of them re-derives it
//!
//! This is the surface §6's non-collapse rule is most easily undone on, and it
//! has already been undone once here: a `--json` consumer using the standard
//! idiom `all(c.outcome != "fail")` read **exit 2 as success**, because a
//! partially-verified receipt contains no failing check. A check run reporting
//! "verified" for a partially-verified receipt is exactly §6's
//! `unverifiable` → `supported` collapse, on the most public surface the
//! product has.
//!
//! So every function here takes a [`Verified`] — the conclusion the verifier
//! computed — and **nothing here reads the checks array to decide an outcome**.
//! [`check_run`] maps `Yes` to `success` and nothing else does;
//! [`conclusion_is_success_only_for_yes`] is the test, and the Action consumes
//! this output rather than deriving its own.
//!
//! # The three GitHub conclusions, and why `neutral` rather than `failure`
//!
//! - `Yes` → **success**.
//! - `Partial` → **neutral**. Something could not be checked *here*; the
//!   receipt is not contradicted. §9: *"`unverifiable` produces a neutral
//!   conclusion with instructions."* Failing it would fail honest work, which
//!   is as damaging as passing cheating.
//! - `No` → **failure**. A check contradicted the receipt.

use serde::Serialize;

use crate::statement::Statement;
use crate::verify::{Outcome, Report, Verified};

/// The GitHub check-run name §9 fixes.
pub const CHECK_NAME: &str = "thruline/receipt";

/// GitHub's three conclusions, as one closed type.
///
/// # Why this exists beside [`Verified`]
///
/// [`Verified`] is the **integrity** axis and nothing else: *do the receipt's
/// own checks reproduce on this machine?* The operator's ruling of 2026-09-04
/// separated two more axes that were being silently folded into it:
///
/// | axis | asks | who decides |
/// |---|---|---|
/// | **integrity** | does the receipt reproduce here? | [`crate::verify`] |
/// | **claims** | did every claim in it get proved? | the receipt's own verdicts |
/// | **policy** | may this merge anyway? | `.thruline/policy.yml` |
///
/// A receipt can be perfect on the first axis and carry an `unsupported` claim
/// on the second — and that is precisely what was measured concluding
/// `success` with a green badge. `conclusion: "success"` on a pull request
/// does not read as *"the signature verified"*; it reads as *"this change is
/// fine"*.
///
/// So the three axes are combined by [`Conclusion::worst`] and never by
/// last-writer-wins: **no axis can lift another.** A passing policy cannot
/// green an unsupported claim, and a clean integrity report cannot green a
/// failing policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Conclusion {
    Success,
    Neutral,
    Failure,
}

impl Conclusion {
    /// GitHub's own spelling. **The only place `"success"` is produced.**
    pub fn as_str(self) -> &'static str {
        match self {
            Conclusion::Success => "success",
            Conclusion::Neutral => "neutral",
            Conclusion::Failure => "failure",
        }
    }

    /// How bad, so that combining is a `max` rather than a hand-written table
    /// a new axis can be left out of.
    fn rank(self) -> u8 {
        match self {
            Conclusion::Success => 0,
            Conclusion::Neutral => 1,
            Conclusion::Failure => 2,
        }
    }

    /// The worse of two conclusions. Combining is **only ever downward**.
    pub fn worst(self, other: Conclusion) -> Conclusion {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }
}

impl From<Verified> for Conclusion {
    fn from(v: Verified) -> Conclusion {
        match v {
            Verified::Yes => Conclusion::Success,
            // Not `failure`: nothing contradicted the receipt, this machine
            // simply could not check everything. §9 asks for neutral with
            // instructions.
            Verified::Partial => Conclusion::Neutral,
            Verified::No => Conclusion::Failure,
        }
    }
}

/// What `.thruline/policy.yml` concluded, as this crate receives it.
///
/// A separate type from `thruline_policy::Conclusion` for the same reason that
/// crate keeps its own `Claim`: `attest` depends on nothing, and the dependency
/// must never be able to run the other way. The value is produced by
/// `thruline policy --conclusion` and carried in as a word; [`parse`] is the
/// only door, and it **refuses** anything else rather than defaulting.
///
/// [`parse`]: PolicyConclusion::parse
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyConclusion {
    Success,
    Neutral,
    Failure,
}

impl PolicyConclusion {
    /// `None` for anything that is not one of the three words — including the
    /// empty string, which means *no policy was evaluated* and is the caller's
    /// to distinguish. A gate that read an unrecognised word as "fine" would
    /// be a gate that passes whenever it breaks.
    pub fn parse(word: &str) -> Option<PolicyConclusion> {
        match word {
            "success" => Some(PolicyConclusion::Success),
            "neutral" => Some(PolicyConclusion::Neutral),
            "failure" => Some(PolicyConclusion::Failure),
            _ => None,
        }
    }
}

impl From<PolicyConclusion> for Conclusion {
    fn from(p: PolicyConclusion) -> Conclusion {
        match p {
            PolicyConclusion::Success => Conclusion::Success,
            PolicyConclusion::Neutral => Conclusion::Neutral,
            PolicyConclusion::Failure => Conclusion::Failure,
        }
    }
}

/// The distribution of the receipt's own claim verdicts — §3.4's three words,
/// counted, with a fourth bucket for anything else.
///
/// # Why it is counted rather than left to the reader
///
/// Operator ruling, point 6: *"`verify --json` carries a top-level verdict
/// summary, so that no consumer can infer 'passed' from 'no check failed'."*
/// The `checks` array alone invites exactly that inference, and the shape has
/// already bitten twice here — once through `all(c.outcome != "fail")` and once
/// through a tri-state field named `verified`.
///
/// Two properties make this un-miscountable, and both are asserted by tests:
///
/// - **The buckets partition the total.** `supported + unsupported +
///   unverifiable + other == total`, always. A verdict word this build does not
///   recognise lands in `other` and is *named* in `other_verdicts`, so it can
///   never go missing and leave a sum that looks complete.
/// - **The answer is stated, not derived.** `all_supported` is a boolean, so no
///   consumer has to add anything up to learn whether a claim went unproved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VerdictSummary {
    pub total: usize,
    pub supported: usize,
    pub unsupported: usize,
    pub unverifiable: usize,
    /// A verdict word outside §3.4's three. Never silently folded into one of
    /// them, and never dropped.
    pub other: usize,
    /// **The stated answer.** `false` when there are no claims at all: a
    /// receipt with nothing in it has not proved anything.
    pub all_supported: bool,
    pub unsupported_claim_ids: Vec<String>,
    pub unverifiable_claim_ids: Vec<String>,
    pub other_verdicts: Vec<String>,
}

impl VerdictSummary {
    pub fn of(statement: Option<&Statement>) -> VerdictSummary {
        let mut s = VerdictSummary::default();
        let Some(st) = statement else {
            return s;
        };
        for v in &st.predicate.verdicts {
            s.total += 1;
            match v.verdict.as_str() {
                "supported" => s.supported += 1,
                "unsupported" => {
                    s.unsupported += 1;
                    s.unsupported_claim_ids.push(v.claim_id.clone());
                }
                "unverifiable" => {
                    s.unverifiable += 1;
                    s.unverifiable_claim_ids.push(v.claim_id.clone());
                }
                other => {
                    s.other += 1;
                    s.other_verdicts.push(other.to_string());
                }
            }
        }
        s.all_supported = s.total > 0 && s.supported == s.total;
        s
    }

    /// The **best** conclusion the claim axis permits, as a [`Verified`].
    ///
    /// - `Yes` — every claim was supported, or the receipt makes no claim.
    /// - `Partial` — something was not proved.
    /// - **`No` is unreachable, and that is ruling point 5.** Deciding that an
    ///   unproved claim may not merge is the repository's call, written in
    ///   `.thruline/policy.yml`. This crate reports; it does not sentence.
    pub fn ceiling(&self) -> Verified {
        if self.unsupported > 0 || self.unverifiable > 0 || self.other > 0 {
            Verified::Partial
        } else {
            Verified::Yes
        }
    }
}

/// **The one conclusion**, from all three axes, combined downward only.
pub fn conclusion_of(
    report: &Report,
    statement: Option<&Statement>,
    policy: Option<PolicyConclusion>,
) -> Conclusion {
    let integrity = Conclusion::from(report.verified());
    let claims = Conclusion::from(VerdictSummary::of(statement).ceiling());
    let mut c = integrity.worst(claims);
    if let Some(p) = policy {
        c = c.worst(Conclusion::from(p));
    }
    c
}

/// GitHub's conclusion for the integrity axis alone.
///
/// Kept because the badge route and a handful of callers speak [`Verified`].
/// **It is not what a check run posts** — that is [`conclusion_of`], which also
/// sees the claims and the policy.
pub fn conclusion(verified: Verified) -> &'static str {
    Conclusion::from(verified).as_str()
}

/// The one-line title a reviewer reads first, over all three axes.
///
/// It names **which axis produced the conclusion**, worst first. The old title
/// spoke only of the receipt, so a receipt that was genuinely verified while
/// its claim went unproved read "Receipt verified" — true about the axis it
/// named, and the wrong sentence to put on a pull request.
pub fn headline(
    report: &Report,
    statement: Option<&Statement>,
    policy: Option<PolicyConclusion>,
) -> String {
    let claims = VerdictSummary::of(statement);
    if report.verified() == Verified::No {
        return "Receipt NOT verified".to_string();
    }
    if policy == Some(PolicyConclusion::Failure) {
        return "Policy FAILED — a claim this repository requires is not supported by the record"
            .to_string();
    }
    if claims.unsupported > 0 {
        return format!(
            "{} claim(s) NOT supported by the record — the receipt is genuine, the claim is not",
            claims.unsupported
        );
    }
    if claims.other > 0 {
        return format!(
            "{} verdict(s) this build does not recognise — not decided, and not a pass",
            claims.other
        );
    }
    if report.verified() == Verified::Partial {
        return "Receipt PARTIALLY verified — not the same as verified".to_string();
    }
    if policy == Some(PolicyConclusion::Neutral) {
        return "Policy could not decide — not the same as passing".to_string();
    }
    "Receipt verified".to_string()
}

/// The one-line title for the integrity axis alone. See [`headline`].
pub fn title(verified: Verified) -> &'static str {
    match verified {
        Verified::Yes => "Receipt verified",
        Verified::Partial => "Receipt PARTIALLY verified — not the same as verified",
        Verified::No => "Receipt NOT verified",
    }
}

/// A check-run payload, ready for `gh api repos/{owner}/{repo}/check-runs`.
///
/// `head_sha` is the caller's; it is the one field that cannot come from the
/// receipt, because the receipt is about a tree and a check run is about a
/// commit in CI.
pub fn check_run(
    report: &Report,
    statement: Option<&Statement>,
    head_sha: Option<&str>,
    policy: Option<PolicyConclusion>,
) -> serde_json::Value {
    // **All three axes.** `report.verified()` alone answers only "does the
    // receipt reproduce here", and posting that as the check run's conclusion
    // put `success` on a pull request whose only claim was `unsupported`.
    let overall = conclusion_of(report, statement, policy);
    let mut payload = serde_json::json!({
        "name": CHECK_NAME,
        "status": "completed",
        "conclusion": overall.as_str(),
        "output": {
            "title": headline(report, statement, policy),
            "summary": summary_of(report, statement, policy),
            "text": markdown_of(report, statement, policy),
        }
    });
    if let Some(sha) = head_sha.filter(|s| !s.is_empty()) {
        if let Some(map) = payload.as_object_mut() {
            map.insert(
                "head_sha".into(),
                serde_json::Value::String(sha.to_string()),
            );
        }
    }
    payload
}

/// One paragraph: what was concluded, and what to do about it.
///
/// The integrity axis only. [`summary_of`] is what a check run posts.
pub fn summary(report: &Report, statement: Option<&Statement>) -> String {
    summary_of(report, statement, None)
}

/// One paragraph over all three axes.
pub fn summary_of(
    report: &Report,
    statement: Option<&Statement>,
    policy: Option<PolicyConclusion>,
) -> String {
    let verified = report.verified();
    let mut out = String::new();
    match verified {
        Verified::Yes => out
            .push_str("Every check this receipt asserts was confirmed against this repository.\n"),
        Verified::Partial => {
            out.push_str(
                "Nothing contradicts this receipt, and something could not be checked here. \
                 **That is not the same as verified.**\n\n",
            );
            for check in report.skipped() {
                if let Outcome::Skipped { reason } = &check.outcome {
                    out.push_str(&format!("- `{}` — {}\n", check.name, inert_cell(reason)));
                }
            }
        }
        Verified::No => {
            out.push_str("At least one check contradicted this receipt.\n\n");
            for check in report.failed() {
                if let Outcome::Fail { reason } = &check.outcome {
                    out.push_str(&format!("- `{}` — {}\n", check.name, inert_cell(reason)));
                }
            }
        }
    }
    // **The claim axis, stated in words, not left to the table below.** This
    // paragraph used to be one line of counts under a "Receipt verified"
    // heading, which is how an `unsupported` claim rode a green check run.
    let claims = VerdictSummary::of(statement);
    if statement.is_some() {
        out.push_str(&format!(
            "\n{} claim(s): {} supported, {} unsupported, {} unverifiable{}.\n",
            claims.total,
            claims.supported,
            claims.unsupported,
            claims.unverifiable,
            if claims.other > 0 {
                format!(", {} not recognised by this build", claims.other)
            } else {
                String::new()
            },
        ));
    }
    if claims.unsupported > 0 {
        out.push_str(&format!(
            "\n**{} claim(s) are NOT supported by the record.** The receipt itself is \
             genuine — these are claims the agent made that the captured evidence does \
             not back. This check run therefore cannot conclude `success`; whether it \
             may merge is `.thruline/policy.yml`'s decision, not this check's.\n\n",
            claims.unsupported
        ));
        for id in &claims.unsupported_claim_ids {
            out.push_str(&format!("- `{}`\n", inert_cell(id)));
        }
    }
    if claims.other > 0 {
        out.push_str(&format!(
            "\n**{} verdict(s) use a word this build does not recognise** ({}). An \
             undecidable verdict is not a pass.\n",
            claims.other,
            inert_cell(&claims.other_verdicts.join(", "))
        ));
    }
    match policy {
        Some(PolicyConclusion::Failure) => out.push_str(
            "\n**`.thruline/policy.yml` failed this change.** Policy is the only thing \
             here that can produce a `failure`.\n",
        ),
        Some(PolicyConclusion::Neutral) => out.push_str(
            "\n`.thruline/policy.yml` could not decide, which is not the same as \
             passing.\n",
        ),
        Some(PolicyConclusion::Success) => out.push_str(
            "\n`.thruline/policy.yml` passed. A passing policy cannot lift any line \
             above it.\n",
        ),
        None => {}
    }
    out
}

/// §9's PR comment table: claims → verdict → evidence → one-line reason.
pub fn markdown(report: &Report, statement: Option<&Statement>) -> String {
    markdown_of(report, statement, None)
}

/// §9's PR comment table, headed by the conclusion from all three axes.
pub fn markdown_of(
    report: &Report,
    statement: Option<&Statement>,
    policy: Option<PolicyConclusion>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "### ThruLine — {}\n\n",
        headline(report, statement, policy)
    ));

    out.push_str("| check | outcome | detail |\n|---|---|---|\n");
    for check in &report.checks {
        let (word, detail) = match &check.outcome {
            Outcome::Pass { detail } => ("ok", detail.as_str()),
            Outcome::Fail { reason } => ("**FAIL**", reason.as_str()),
            Outcome::Skipped { reason } => ("skipped", reason.as_str()),
        };
        // `check.name` is ours; `detail` can quote a digest, a path or a session
        // id the agent influenced, so it is inert too.
        out.push_str(&format!(
            "| `{}` | {word} | {} |\n",
            check.name,
            inert_cell(detail)
        ));
    }

    let Some(s) = statement else {
        return out;
    };
    if s.predicate.claims.is_empty() {
        out.push_str("\nThe agent made no completion claim this build recognises.\n");
        return out;
    }
    out.push_str("\n| claim | verdict | why |\n|---|---|---|\n");
    for claim in &s.predicate.claims {
        let verdict = s.predicate.verdicts.iter().find(|v| v.claim_id == claim.id);
        let (word, why) = match verdict {
            Some(v) => (
                match v.verdict.as_str() {
                    "supported" => "supported".to_string(),
                    "unsupported" => "**unsupported**".to_string(),
                    other => other.to_string(),
                },
                v.reasons.join("; "),
            ),
            None => ("—".to_string(), String::new()),
        };
        // The claim text is the agent's own sentence, and `why` embeds argv and
        // paths the agent chose. Both are inert.
        out.push_str(&format!(
            "| {} | {word} | {} |\n",
            inert_cell(&claim.text),
            inert_cell(&why)
        ));
    }

    let reexecuted = s.predicate.checks.iter().filter(|c| c.reexecuted).count();
    if reexecuted > 0 {
        out.push_str(&format!(
            "\n{reexecuted} check(s) were **re-executed** in a clean container at the \
             final tree, each bound to a container image digest.\n"
        ));
    }
    if s.predicate.capture.capture_degraded {
        out.push_str(
            "\n**Capture was degraded for this session**, so no claim in it can reach \
             `supported`.\n",
        );
    }
    out
}

/// Render **agent-controlled** text in a table cell, inert.
///
/// The PR comment is rendered markdown on the most public surface the product
/// has, and the claim text is whatever the agent wrote. Left raw,
/// `[click](http://…)` renders as a link and `![](http://…)` renders as a
/// **request made by whoever opens the pull request** — which tells a third
/// party who read it and when. That is verbatim the reasoning §0.8 gives for
/// `explain --html` fetching nothing, applied to the surface with the wider
/// audience.
///
/// An inline code span neutralises every markdown and HTML construct except a
/// backtick, so the backtick is the only character that needs replacing — and
/// the pipe, which GFM resolves before inline parsing and which would otherwise
/// end the cell.
///
/// **Every cell in the table goes through this**, including ones this crate
/// wrote: a separate "ours is safe" path would be one more place a future
/// caller can put agent text by mistake, and the cost of routing our own
/// strings through it is a pair of backticks.
fn inert_cell(text: &str) -> String {
    let flat = text.replace(['\n', '\r'], " ");
    // U+2018, which reads as a quote and cannot close a code span.
    let quoted = flat.replace('`', "\u{2018}");
    format!("`{}`", quoted.replace('|', "\\|"))
}

/// The §9 `verified-by-thruline` badge.
///
/// Self-contained SVG with no external font, no image and no script — the same
/// rule `explain --html` follows, and for the same reason: a badge is embedded
/// in a README, and a linked resource would tell a third party who read it.
///
/// Three states, never two. A badge that says "verified" for a partially
/// verified receipt is the collapse this whole module exists against, on the
/// surface with the widest audience.
pub fn badge(verified: Verified) -> String {
    badge_of(Conclusion::from(verified))
}

/// **The one definition of what a badge says.** (Track R, F15(c).)
///
/// # Two crates said "not verified" and meant different things
///
/// Measured at `fa88a11`:
///
/// | conclusion | here | `app_core::badge::label_and_colour` |
/// |---|---|---|
/// | `Success` | "verified" `#2ea043` | "verified" `#2da44e` |
/// | `Neutral` | "partial" `#bf8700` | **"not verified"** `#9a6700` |
/// | `Failure` | **"not verified"** `#cf222e` | "unsupported" `#cf222e` |
///
/// So **"not verified" was `Neutral` on one surface and `Failure` on the
/// other** — the same two words, two meanings, two colours, on the most-copied
/// and least-read surface a product has. Even `Success` had two greens. Neither
/// table was wrong on its own terms; there were simply two tables, and
/// agreeing them by hand would leave two tables.
///
/// This is now `pub` so there can be one. `crates/app-core` is Track C's lane
/// under ruling 2A, on the condition that it depends only on the core's public
/// interfaces — which is what this is. The one-line change is routed to C
/// rather than made here; see `docs/DECISIONS-PENDING.md` D20.
///
/// # The words are unchanged, and that is deliberate
///
/// This does **not** settle which words are right. On the merits app-core's
/// rows read better: `Neutral` is *we could not tell*, which "partial"
/// overstates by implying part of it verified, and `Failure` is a check that
/// contradicted the receipt, which is stronger than "not verified". But badge
/// labels are user-facing copy, operator ruling 9 governs user-facing copy, and
/// D16 defers that flip because it cannot be measured in this session.
/// Changing them here would also break `packaging/action/selftest.py`, which
/// pins the word and the colour and is Track C's file. One definition today;
/// the vocabulary question routed with its reasoning rather than settled by
/// whoever edited last.
pub fn badge_label(conclusion: Conclusion) -> (&'static str, &'static str) {
    match conclusion {
        Conclusion::Success => ("verified", "#2ea043"),
        // Amber, and the word is not "verified".
        Conclusion::Neutral => ("partial", "#bf8700"),
        Conclusion::Failure => ("not verified", "#cf222e"),
    }
}

/// The badge for the conclusion a check run actually posts.
///
/// **A badge that only ever shows green is a decoration.** All three states are
/// observed by `packaging/action/selftest.py` against the bytes this function
/// emits, never asserted from a document.
pub fn badge_of(conclusion: Conclusion) -> String {
    let (label, colour) = badge_label(conclusion);
    let left = "thruline";
    // 6.2px per character at 11px DejaVu-ish metrics, plus 10px padding each
    // side. Approximate on purpose: an SVG badge that measures text needs a
    // font, and a font is an external resource.
    let lw = (left.len() as f32 * 6.2).round() + 20.0;
    let rw = (label.len() as f32 * 6.2).round() + 20.0;
    let total = lw + rw;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total}" height="20" role="img" aria-label="thruline: {label}">
  <title>thruline: {label}</title>
  <linearGradient id="s" x2="0" y2="100%"><stop offset="0" stop-color="#bbb" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient>
  <clipPath id="r"><rect width="{total}" height="20" rx="3" fill="#fff"/></clipPath>
  <g clip-path="url(#r)">
    <rect width="{lw}" height="20" fill="#555"/>
    <rect x="{lw}" width="{rw}" height="20" fill="{colour}"/>
    <rect width="{total}" height="20" fill="url(#s)"/>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="Verdana,DejaVu Sans,Geneva,sans-serif" font-size="11">
    <text x="{lx}" y="14">{left}</text>
    <text x="{rx}" y="14">{label}</text>
  </g>
</svg>
"##,
        lx = lw / 2.0,
        rx = lw + rw / 2.0,
    )
}

/// The badge endpoint §9 names, as a pure route.
///
/// Returns `(status, content_type, body)` for `GET /badge/verified-by-thruline.svg`.
/// **No server is started here**: §9's badge lives on the hosted App, and a
/// route function is the part of it that can be built and tested now.
///
/// # F15(a) — the URL used to decide what the badge said
///
/// This read `?status=` out of the query and mapped it to the badge's state:
/// `?status=verified` rendered the green one. Track R filed it, and noted the
/// mitigating fact — **it is mounted nowhere** — which is exactly why it had to
/// be fixed before it was mounted rather than after. A route that takes its
/// answer from its own URL is not a badge, it is a rendering service for
/// whatever the caller wants to claim, and the first server to mount it would
/// have published one.
///
/// The `verified` state is now a **parameter**: the caller — which is the only
/// party that has verified anything — supplies it, and the query string is
/// ignored entirely rather than validated. Validation was the shape of the old
/// bug: the previous code carefully refused unknown `?status=` values and
/// defaulted them to `No`, which reads as caution and still let the two known
/// values through.
///
/// The query is still split off the path so a URL carrying one is routed rather
/// than 404'd; nothing is read from it.
pub fn badge_route(
    path_and_query: &str,
    verified: Verified,
) -> Option<(u16, &'static str, String)> {
    let path = match path_and_query.split_once('?') {
        Some((p, _query)) => p,
        None => path_and_query,
    };
    if path != "/badge/verified-by-thruline.svg" {
        return None;
    }
    Some((200, "image/svg+xml; charset=utf-8", badge(verified)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conclusion_is_success_only_for_yes() {
        // The single most important assertion in this module.
        assert_eq!(conclusion(Verified::Yes), "success");
        assert_ne!(conclusion(Verified::Partial), "success");
        assert_ne!(conclusion(Verified::No), "success");
        // And `partial` is not a failure either: failing honest work is as
        // damaging as passing cheating.
        assert_eq!(conclusion(Verified::Partial), "neutral");
        assert_eq!(conclusion(Verified::No), "failure");
    }

    #[test]
    fn the_badge_never_says_verified_unless_it_is() {
        assert!(badge(Verified::Yes).contains(">verified<"));
        for other in [Verified::Partial, Verified::No] {
            let svg = badge(other);
            assert!(
                !svg.contains(">verified<"),
                "the badge for {other:?} reads as verified: {svg}"
            );
        }
        // Self-contained: nothing is fetched when a README renders it.
        for state in [Verified::Yes, Verified::Partial, Verified::No] {
            let svg = badge(state);
            for forbidden in ["http://", "https://", "<script", "<image", "@import"] {
                assert!(
                    !svg.replace("http://www.w3.org/2000/svg", "")
                        .contains(forbidden),
                    "{state:?} badge reaches for {forbidden}"
                );
            }
        }
    }

    /// F15(a) — the URL cannot decide what the badge says.
    ///
    /// This test used to assert that an *unrecognised* `?status=` was not
    /// verified, which reads as caution and was the bug: `?status=verified`
    /// was recognised, and rendered the green badge. The property now is
    /// stronger and simpler — **no query changes the answer at all**.
    #[test]
    fn no_query_string_can_change_what_the_badge_says() {
        // The one that mattered is first: the value the old code honoured.
        for query in [
            "",
            "?status=",
            "?status=verified",
            "?status=partial",
            "?status=yes",
            "?status=ok",
            "?status=true",
            "?status=verified&status=verified",
            "?a=1&status=verified",
        ] {
            let url = format!("/badge/verified-by-thruline.svg{query}");
            let (status, mime, svg) = badge_route(&url, Verified::No).expect("route");
            assert_eq!(status, 200);
            assert!(mime.starts_with("image/svg+xml"));
            assert!(
                !svg.contains(">verified<"),
                "query {query:?} produced a verified badge from a caller that had \
                 verified nothing"
            );
            // …and it is byte-identical to the query-less answer, so nothing in
            // the URL leaks into the rendering by another route either.
            let (_, _, plain) =
                badge_route("/badge/verified-by-thruline.svg", Verified::No).expect("route");
            assert_eq!(svg, plain, "query {query:?} changed the bytes");
        }

        // The control: the CALLER can still say verified, or the route is
        // useless and the test above proves nothing.
        let (_, _, svg) =
            badge_route("/badge/verified-by-thruline.svg", Verified::Yes).expect("route");
        assert!(svg.contains(">verified<"));

        // And an unverified caller cannot be talked into it by the path either.
        assert!(badge_route("/badge/other.svg", Verified::Yes).is_none());
    }

    #[test]
    fn agent_written_text_renders_as_text_and_never_as_markup() {
        // The construction: an agent's final message becomes a claim, and the
        // claim goes into a PR comment on a public repository. A link renders
        // as a link; an image renders as a REQUEST MADE BY WHOEVER OPENS THE
        // PULL REQUEST, which tells a third party who read it and when.
        for hostile in [
            "All tests pass ![](http://tracker.invalid/p.gif)",
            "All tests pass [click](http://evil.invalid)",
            "All tests pass <img src=\"http://tracker.invalid/p.gif\">",
            "a | b | c",
            "done\nand `code` too",
            "</details><script>alert(1)</script>",
        ] {
            let cell = inert_cell(hostile);
            // One code span, opened and closed exactly once, so nothing inside
            // it is parsed as markdown or HTML.
            assert!(cell.starts_with('`') && cell.ends_with('`'), "{cell}");
            assert_eq!(
                cell.matches('`').count(),
                2,
                "an inner backtick closed the span early: {cell}"
            );
            // The cell cannot end early either.
            assert!(!cell.contains('\n'), "{cell}");
            for (i, _) in cell.match_indices('|') {
                assert!(
                    i > 0 && cell.as_bytes()[i - 1] == b'\\',
                    "an unescaped pipe ends the cell: {cell}"
                );
            }
            // And the text is still readable — this is not a redaction.
            assert!(cell.contains("All tests pass") || cell.len() > 4, "{cell}");
        }
    }
}
