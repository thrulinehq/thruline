//! BUILD-SPEC §3.4 — verdicts and findings.
//!
//! # The rule this file exists to make structural
//!
//! > *"`unverifiable`: ThruLine could not observe enough to decide. **Never
//! > collapse `unverifiable` into `supported`.**"* (§6)
//!
//! [`Verdict`] has no `Default`. A defaulted verdict would be `Supported` under
//! any derive that picks the first variant, and the one mistake this crate must
//! make impossible is turning "I could not tell" into "everything is fine" —
//! the exact failure the product exists to catch, committed by the product.
//!
//! The same reasoning shapes [`Verdict::worst`]: combining verdicts always
//! moves *away* from `supported`, never toward it.

use serde::{Deserialize, Serialize};

use crate::binding::Binding;

/// §3.4 `verdict`. **No `Default`** — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    // Ordered worst-last so `max` is "the more cautious answer".
    Supported,
    Unverifiable,
    Unsupported,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Supported => "supported",
            Verdict::Unsupported => "unsupported",
            Verdict::Unverifiable => "unverifiable",
        }
    }

    /// The more cautious of two verdicts. Never returns `Supported` unless
    /// both are.
    pub fn worst(self, other: Verdict) -> Verdict {
        self.max(other)
    }
}

/// §6.7 severities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Fail,
}

/// One thing a detector noticed.
///
/// # The cap rides on the finding (R8)
///
/// Operator ruling, 2026-09-06: *"Every finding carries its language, and every
/// severity-raising site consults the cap — enforced by the type, not by
/// emission order. A cap that depends on which crate runs first is a cap that
/// will be silently lost on the next refactor."*
///
/// The defect it rules on was **demonstrated, not predicted**. A language pack
/// over its ambiguity budget caps its findings at `warn` at the point they are
/// emitted; `tests_pass::escalate` then raised `skip_marker_added` to `fail`
/// downstream, and a Ruby claim was refuted by a detector the ruling had
/// declared ineligible to refute, in the first run after the cap landed.
///
/// So the ceiling is a property of the **value**, set with the language by the
/// pack that owns it, and [`Finding::raised_to`] is the only way a severity
/// increases. Ordering cannot lose it, because there is no order to get wrong:
/// a capped finding that reaches a raising site is clamped by the site's own
/// call, and a capped finding that reaches no raising site was already clamped
/// when it was named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// **Read freely; do not assign.** The one thing this type cannot yet make
    /// impossible: `f.severity = Severity::Fail` compiles. It stays public
    /// because two `#[cfg(test)]` lines in `crates/langpacks` read it as a
    /// field and that crate is another track's lane. `crates/verify/tests/
    /// severity_cap_holds.rs` censuses the assignment instead, over this
    /// crate's own source and the CLI's, so the hole is watched rather than
    /// merely regretted.
    pub severity: Severity,
    /// The stable name a report and a test refer to: `narrowed_run`,
    /// `assertion_weakened`, `tests_ran_before_final_edit`.
    pub detector: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub detail: String,
    /// The language pack that emitted this, if any. `None` is a finding of
    /// verify's own, which is under no pack's ceiling.
    ///
    /// **Not serialised.** Whether the receipt records a finding's language is
    /// a schema question, and it belongs with D14's rather than arriving as a
    /// side effect of a severity fix — `$defs/finding` in
    /// `spec/execution-check-v0.1.schema.json` is `additionalProperties:
    /// false`, so it would be a receipt change and must be recorded as one.
    #[serde(skip)]
    pub language: Option<String>,
    /// The ceiling [`Finding::raised_to`] will not cross. Private: a cap that
    /// anything outside this module can widen is not a cap.
    #[serde(skip)]
    cap: Option<Severity>,
}

impl Finding {
    pub fn new(severity: Severity, detector: &str, detail: impl Into<String>) -> Finding {
        Finding {
            severity,
            detector: detector.to_string(),
            path: None,
            detail: detail.into(),
            language: None,
            cap: None,
        }
    }

    pub fn at(mut self, path: impl Into<String>) -> Finding {
        self.path = Some(path.into());
        self
    }

    /// Names the pack that emitted this finding and the ceiling that pack is
    /// under, and applies the ceiling immediately — a capped finding cannot
    /// even be born above it.
    ///
    /// `cap` is `None` for a pack under no ceiling; that is not the same as
    /// `Some(Severity::Fail)`, and both are spelled so a pack cannot express
    /// "capped" by accident.
    pub fn from_pack(mut self, language: impl Into<String>, cap: Option<Severity>) -> Finding {
        self.language = Some(language.into());
        self.cap = cap;
        self.clamp();
        self
    }

    /// **The only way a finding's severity goes up.** Never lowers: a site that
    /// wants to say "the run showed this changed nothing" builds a new finding
    /// rather than demoting this one.
    ///
    /// Both of verify's severity-raising sites go through it —
    /// `tests_pass::escalate` and `special_casing::escalate_with_test_findings`
    /// — and neither needs to know whether a cap exists, which is the point:
    /// the site asks for a severity and the value decides what it gets.
    pub fn raised_to(mut self, severity: Severity) -> Finding {
        self.severity = self.severity.max(severity);
        self.clamp();
        self
    }

    /// Lower a severity. Never raises, and consults no cap — a ceiling has
    /// nothing to say about going down.
    ///
    /// It exists so that **no** site outside this module assigns `severity` at
    /// all, which is what lets
    /// `crates/verify/tests/severity_cap_holds.rs`'s census be absolute. A
    /// census with one legitimate exception is a census whose next exception
    /// gets waved through on the precedent.
    pub fn lowered_to(mut self, severity: Severity) -> Finding {
        self.severity = self.severity.min(severity);
        self
    }

    /// This finding's severity.
    ///
    /// # Additive, on purpose (Track G, R9)
    ///
    /// The field is still `pub`, and this accessor is beside it rather than
    /// instead of it. Making the field private in the same commit would be a
    /// cross-lane break from a change that looks local: `crates/langpacks`
    /// reads `.severity` as a field in **twelve** places, nearly all of the
    /// shape `.map(|f| (f.detector.clone(), f.severity))` — building
    /// detector/severity pairs so a family test can assert *which* detector
    /// fired at *what* severity, which [`Finding::is_fail`] cannot express.
    ///
    /// **My own note undercounted them and said two.** The two were the
    /// `#[cfg(test)]` reads inside `crates/langpacks/src`; the other twelve are
    /// integration tests under `crates/langpacks/tests/`, which do not exist at
    /// this repository's head yet and which no `#[cfg(test)]` census could have
    /// seen. A count taken over one tree and reported as a count over the
    /// program is the same error as a stale performance figure.
    ///
    /// The sequence, so the field can close without breaking anyone: this
    /// lands; G converts its twelve and confirms; **then** the field goes
    /// private and `severity_cap_holds.rs`'s assignment census is deleted
    /// rather than kept, because a compiler check makes a search redundant.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The ceiling this finding is under, so a report can explain a severity
    /// that did not rise.
    pub fn severity_cap(&self) -> Option<Severity> {
        self.cap
    }

    fn clamp(&mut self) {
        if let Some(cap) = self.cap {
            if self.severity > cap {
                self.severity = cap;
            }
        }
    }

    pub fn is_fail(&self) -> bool {
        self.severity == Severity::Fail
    }
}

/// §3.4 — the verdict record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictRecord {
    pub claim_id: String,
    pub verdict: Verdict,
    /// `tests_pass@1` — the verifier and its version, so a stored verdict says
    /// which code produced it.
    pub verifier: String,
    pub evidence: Vec<String>,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub findings: Vec<Finding>,
    /// **Which §6.1 rule let the evidence count**, structurally.
    ///
    /// It used to exist only as a sentence inside `reasons`, and serialised as
    /// `null`. §6.1 says to *record which rule applied*: rule 2 is a
    /// relaxation, so a reader who cannot tell it from rule 1 cannot audit the
    /// verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<Binding>,
}

impl VerdictRecord {
    pub fn new(claim_id: &str, verifier: &str, verdict: Verdict) -> VerdictRecord {
        VerdictRecord {
            claim_id: claim_id.to_string(),
            verdict,
            verifier: verifier.to_string(),
            evidence: Vec::new(),
            reasons: Vec::new(),
            warnings: Vec::new(),
            findings: Vec::new(),
            binding: None,
        }
    }

    pub fn with_binding(mut self, binding: Binding) -> VerdictRecord {
        self.binding = Some(binding);
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> VerdictRecord {
        self.reasons.push(reason.into());
        self
    }

    /// Add a finding, and let it move the verdict — never toward `supported`.
    pub fn with_finding(mut self, finding: Finding) -> VerdictRecord {
        if finding.is_fail() {
            self.verdict = self.verdict.worst(Verdict::Unsupported);
        } else if finding.severity == Severity::Warn {
            self.warnings.push(finding.detail.clone());
        }
        self.findings.push(finding);
        self
    }
}
