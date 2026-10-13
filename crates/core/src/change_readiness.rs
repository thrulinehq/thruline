//! T-3-5 (`B7.6`–`B7.12`, `Q1`, `Q3`, lens rule 5) — **"Ready for approval"**,
//! computed once, over records, and never as a colour.
//!
//! ## The sentence this module is responsible for
//!
//! "Ready for approval for version `<v>`" is the strongest thing ThruLine ever
//! says. Lens rule 5 puts two conditions on it that are not presentation:
//! it **lists what it rests on**, and **missing evidence is shown as missing,
//! never as a pass**. Both are held here rather than on a screen, because a
//! screen that decides this has already made it possible for two screens to
//! decide it differently.
//!
//! ## One pure function
//!
//! [`compute`] takes [`ReadinessInput`] — typed records, no ledger, no clock,
//! no environment — and returns [`Readiness`]. Same input, same answer, for
//! ever: that is the replay determinism `Q1` asks for, and it is what makes
//! every unsatisfying class of `B7.6` testable as a table.
//!
//! [`readiness`] is the thin gatherer that reads the records out of a
//! [`CoverageView`] the caller already built, plus whether this Change has
//! written down what it will examine. It decides nothing.
//!
//! ## The rule, in full
//!
//! Ready **iff all** of:
//!
//! 1. an accepted scope version exists **and holds at least one criterion**
//!    (`B7.7`: an empty set is never an all-clear — with nothing accepted,
//!    nothing counts as done yet, and the answer is that sentence rather than
//!    a pass);
//! 2. this Change has an examined-inputs record — what *will* be looked at
//!    (`B7.12` candidate identity, and it is **non-waivable**);
//! 3. every criterion obligation is **satisfied by one admissible item** (see
//!    below) or carries an explicit, recorded waiver;
//! 4. every review obligation the **accepted policy actually records** is
//!    satisfied or explicitly waived under that policy. ThruLine does not
//!    invent a review requirement that nobody wrote down, and it does not
//!    assume one was met: "recorded, never assumed" cuts both ways.
//!
//! ## What counts as one admissible item (`B7.6`, `Q3`)
//!
//! All six, together:
//!
//! * it **bears** on the line (a run that checked nothing does not);
//! * its observed result is **`Proven`** — and `Proven` is the engine's
//!   existing, conservative judgement: a test command that exited 0 is
//!   `Recorded`, because this store cannot tell a full suite from zero
//!   tests, and `Q3` forbids guessing towards satisfaction;
//! * it **applies to this version** (`applies == Some(true)`): a result about
//!   another version is not a smaller result, it is a result about something
//!   else;
//! * the record says it **still applies** (`fresh == Some(true)`): an
//!   applicability assessment exists for it and found nothing that moved.
//!   Stale evidence (`Some(false)`) never satisfies anything, and neither does
//!   evidence whose applicability was **never assessed** (`None`) — it does not
//!   count toward Ready, the same way Couldn't check does not (R-2026-09-28-25,
//!   R-2026-09-27-14). Such a line is named [`Unsatisfied::NeverAssessed`] by
//!   [`never_assessed`], the one rule every surface reads;
//! * its execution state never **contradicts** completion: `completed`, or no
//!   execution row at all for a run recorded before the supervisor existed.
//!   `running`, `cancelled`, `interrupted` and `blocked` never satisfy
//!   anything — that is T-3-4's rule, re-asserted here so readiness cannot
//!   route around it.
//!
//! ## Waivers and not-applicable (`B7.9`–`B7.11`)
//!
//! An [`ObligationWaiver`] is an **appended record** with an actor, the policy
//! authority it rests on, a rationale and a time. [`ObligationWaiver::validate`]
//! refuses one that is missing any of them, and refuses one aimed at any of
//! [`NON_WAIVABLE`] — candidate identity, decision authority and packet
//! consistency are not waivable by construction, so there is no code path that
//! could be persuaded otherwise.
//!
//! An obligation set aside stops *blocking*, and it is **never counted as
//! evidence**: it appears in [`Readiness::waived`] / [`Readiness::not_applicable`],
//! never in [`Readiness::rests_on`], and its own sentence names the person and
//! the reason. A reader who sees only "ready" and not what it rests on has
//! been failed by the surface, not by this module.
//!
//! ## The scope cut T-3-5 stated, and what T-5-3 did with it (`B7.18`-`B7.22`)
//!
//! T-3-5's own doc said here that the waiver **store** and its two write
//! routes were not built, that the caller supplied an empty list, and that no
//! DDL changed. That was true then and is false now; it is corrected in place
//! rather than left standing. T-5-3 added the table (**DDL — that task
//! required `thruline upgrade`**), [`append_waiver`], [`waivers`] and
//! [`waive`], and the two write routes plus `thruline change waive` call that
//! one function. The table shipped as `change_dispositions` and was renamed
//! to `change_obligation_waivers` hours later by R-2026-09-24-22, with a
//! migration (`Ledger::migrate_obligation_waiver_rename`): "disposition"
//! names what happened to a *finding* in the review layer, and one word for
//! two things is how a query ends up reading the wrong table. [`compute`] is
//! untouched by any of it: it still decides from typed records and nothing
//! else.
//!
//! Two rules the store does not relax. A waiver **clears** an obligation and
//! is **never** evidence, so [`Readiness::with_exceptions`] is its own field
//! and a waived line is still absent from [`Readiness::rests_on`]; and a
//! Change whose accepted profile records no exception path
//! ([`exception_authority`]) cannot have one written at all.

use serde::Serialize;

use crate::change_evidence::{CoverageState, CoverageView, EvidenceKind, Observed};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::run_execution::ExecutionState;

/// Bump when a served field below is added or changed. **2** since T-5-3:
/// `with_exceptions` on [`Readiness`] and `id` on [`ObligationWaiver`].
pub const READINESS_SCHEMA_VERSION: u32 = 2;

/// **`B7.12`.** The three prerequisites no policy may waive. Refused by
/// [`ObligationWaiver::validate`], so the refusal is a property of the record type
/// and not of whichever caller remembered to check.
pub const NON_WAIVABLE: &[&str] = &[
    "candidate_identity",
    "decision_authority",
    "packet_consistency",
];

/// What kind of thing an obligation is. Closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationKind {
    /// One accepted line, and the evidence the record holds for it.
    Criterion,
    /// The Change has written down what will be examined (`B7.12`).
    CandidateIdentity,
    /// The accepted policy records that this needs a review.
    Review,
}

/// What became of one obligation. Four outcomes, and **none of them is a
/// colour**: `Waived` and `NotApplicable` are separate from `Satisfied` at the
/// type level, so no surface can flatten them into one another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Satisfied,
    NeedsEvidence,
    Waived,
    NotApplicable,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::NeedsEvidence => "needs_evidence",
            Self::Waived => "waived",
            Self::NotApplicable => "not_applicable",
        }
    }

    /// Does this outcome stop the obligation blocking readiness? A waiver
    /// does; it still is not evidence, and [`Outcome::is_evidence`] is the
    /// other half of that distinction.
    pub fn clears(self) -> bool {
        !matches!(self, Self::NeedsEvidence)
    }

    /// Is this outcome the record backing the line? **Only one is.**
    pub fn is_evidence(self) -> bool {
        matches!(self, Self::Satisfied)
    }
}

/// Why an obligation is not satisfied, as a closed set of copy slugs. Every
/// unsatisfying class of `B7.6` has one, so a screen never has to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unsatisfied {
    /// Nothing in the record bears on this line.
    NothingLooked,
    /// Nothing is linked to this line, and the Change holds a completed,
    /// bearing, Proven check run that no line is linked to (RUN-LINK-TRUTH).
    /// The run does not count for the line until a named person links it; the
    /// reason says so instead of "No test ran for this."
    UnlinkedRun,
    /// Something is linked and none of it settled anything.
    SettledNothing,
    /// The record holds the opposite.
    NotProven,
    /// The check this line asks for cannot run on this machine.
    ToolAbsent,
    /// Everything that bears on it was examined on another version.
    OtherVersion,
    /// The evidence was proven, and the record has moved since.
    Stale,
    /// Proven on this version, and whether it still applies was never
    /// assessed (R-2026-09-28-25). Named only when assessing it is the one
    /// thing standing between this line and satisfied: see [`never_assessed`].
    NeverAssessed,
    /// Proven on this version, whether it still applies was never assessed,
    /// and no `change prepare` can assess it: nothing on record says what
    /// the files were when it was checked (NEVER-ASSESSED-NOT-READY-R2).
    /// Named only for a line [`never_assessed`] names, when every item that
    /// would settle it reads `assessable == Some(false)`.
    CouldNotAssess,
    /// The run was stopped, cut off or never started.
    Stopped,
    /// A run finished and this store cannot say what it covered.
    Unknown,
    /// This Change has not written down what will be examined.
    InputsMissing,
    /// The accepted policy records a review, and none is written down.
    ReviewMissing,
}

impl Unsatisfied {
    /// The copy slug. The sentence is looked up by a surface, never built here.
    pub fn slug(self) -> &'static str {
        match self {
            Self::NothingLooked => "readiness_nothing_looked",
            Self::UnlinkedRun => "readiness_unlinked_run",
            Self::SettledNothing => "readiness_settled_nothing",
            Self::NotProven => "readiness_not_proven",
            Self::ToolAbsent => "readiness_tool_absent",
            Self::OtherVersion => "readiness_other_version",
            Self::Stale => "readiness_stale",
            Self::NeverAssessed => "readiness_never_assessed",
            Self::CouldNotAssess => "readiness_could_not_assess",
            Self::Stopped => "readiness_stopped",
            Self::Unknown => "readiness_unknown",
            Self::InputsMissing => "readiness_inputs_missing",
            Self::ReviewMissing => "readiness_review_missing",
        }
    }
}

/// An **appended record** saying a person set one obligation aside without
/// evidence: waived it, or said it does not apply here.
///
/// It is never an edit of anything (lens rule 13). Every field below is
/// required, because an unattributed waiver with no authority behind it is
/// indistinguishable from the tool having decided on its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObligationWaiver {
    /// The appended record's own id. Empty on a record that has not been
    /// stored yet — [`append_waiver`] mints one — and the id an approval's exception
    /// row points back at once it has (`B10.9`).
    pub id: String,
    /// The obligation this is about, by [`Obligation::id`].
    pub obligation_id: String,
    /// Waived, or does not apply here.
    pub kind: WaiverKind,
    /// Who. An unattributed waiver is refused.
    pub actor: String,
    /// The accepted policy this rests on, by name. Refused when empty: a
    /// waiver with no authority is a person overruling the record silently.
    pub authority: String,
    /// Why, in their own words. Shown wherever the obligation is shown.
    pub rationale: String,
    pub at: String,
}

/// The two waivers, and there is no third.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaiverKind {
    /// The obligation stands and is being accepted without its evidence.
    Waived,
    /// The obligation does not apply to this Change at all.
    NotApplicable,
}

impl WaiverKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waived => "waived",
            Self::NotApplicable => "not_applicable",
        }
    }

    fn outcome(self) -> Outcome {
        match self {
            Self::Waived => Outcome::Waived,
            Self::NotApplicable => Outcome::NotApplicable,
        }
    }

    /// The copy slug for the sentence that renders it. Neither reads as a
    /// pass, and `readiness_copy.rs` asserts that mechanically.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Waived => "readiness_waived",
            Self::NotApplicable => "readiness_not_applicable",
        }
    }
}

impl ObligationWaiver {
    /// **`B7.9`–`B7.12`.** Refuse anything that is not a complete, attributed,
    /// authorised record, and refuse outright anything aimed at one of
    /// [`NON_WAIVABLE`].
    pub fn validate(&self) -> Result<()> {
        if self.obligation_id.trim().is_empty() {
            return Err(CoreError::Config(
                "a waiver has to name the obligation it is about".into(),
            ));
        }
        if NON_WAIVABLE.contains(&self.obligation_id.trim()) {
            return Err(CoreError::Config(format!(
                "{} cannot be waived: it is one of the prerequisites no policy may set aside",
                self.obligation_id.trim()
            )));
        }
        if self.actor.trim().is_empty() {
            return Err(CoreError::Config(
                "a waiver has to have a name on it".into(),
            ));
        }
        if self.authority.trim().is_empty() {
            return Err(CoreError::Config(
                "a waiver has to name the accepted policy it rests on".into(),
            ));
        }
        if self.rationale.trim().is_empty() {
            return Err(CoreError::Config(
                "a waiver has to say why, in words a person can read".into(),
            ));
        }
        Ok(())
    }
}

/// One thing that has to be true before this version can be approved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Obligation {
    /// Stable id: `criterion:<n>`, `candidate_identity`, `review`.
    pub id: String,
    pub kind: ObligationKind,
    pub criterion_index: Option<u32>,
    /// The line's own words, as the record holds them (already masked).
    pub text: String,
    pub outcome: Outcome,
    pub outcome_slug: &'static str,
    /// Why it is not satisfied. `None` when it is.
    pub unsatisfied: Option<Unsatisfied>,
    pub unsatisfied_slug: Option<&'static str>,
    /// The evidence's own name, when one item satisfies it. Never a count.
    pub evidence_name: Option<String>,
    /// RUN-LINK-TRUTH — who linked it, when the item that satisfies this line
    /// counts through a person's link (basis `manual`). A line never rests on
    /// a person's link without that person named beside it. `None` for every
    /// other basis and for an unsatisfied line.
    pub linked_by: Option<String>,
    /// The recorded waiver, when there is one. Rendered wherever this
    /// obligation is rendered.
    pub waiver: Option<ObligationWaiver>,
    /// `false` for the three prerequisites of `B7.12`.
    pub waivable: bool,
}

/// One thing the readiness sentence rests on — a **satisfied** obligation and
/// nothing else. A waived obligation is deliberately absent from this list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestsOn {
    pub obligation_id: String,
    pub text: String,
    pub evidence_name: Option<String>,
    /// Who linked it, when it counts through a person's link. See
    /// [`Obligation::linked_by`].
    pub linked_by: Option<String>,
}

/// What the whole answer is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Readiness {
    pub schema_version: u32,
    pub change_id: String,
    /// The one boolean, and the only place it is decided.
    pub ready: bool,
    /// **`B7.22`.** Ready, and resting in part on something nobody checked.
    /// Never true while `ready` is false, and never folded into it.
    pub with_exceptions: bool,
    pub scope_version: Option<u32>,
    /// The version this readiness is **about**. `ready` is meaningless without
    /// it, so a ready answer never has this empty.
    pub target_version: Option<String>,
    /// The copy slug for the headline sentence.
    pub headline_slug: &'static str,
    pub obligations: Vec<Obligation>,
    /// What a ready answer rests on, listed. Empty on a not-ready answer.
    pub rests_on: Vec<RestsOn>,
    /// The obligations still needing evidence, in accepted order.
    pub missing: Vec<String>,
    pub waived: Vec<String>,
    pub not_applicable: Vec<String>,
}

impl Readiness {
    /// How many obligations still need evidence. A count, never a percentage:
    /// a percentage of a proof is not a thing.
    pub fn missing_count(&self) -> usize {
        self.missing.len()
    }
}

/// The typed records [`compute`] decides from. Nothing else reaches it.
#[derive(Debug, Clone)]
pub struct ReadinessInput<'a> {
    pub change_id: &'a str,
    pub coverage: &'a CoverageView,
    /// The version the Change is bound to, from the record.
    pub target_version: Option<String>,
    /// Does an examined-inputs record exist for this Change (`B7.12`)?
    pub has_examined_inputs: bool,
    /// Does the **accepted policy** record that a review is required? Read out
    /// of the accepted check profile; never assumed in either direction.
    pub review_required: bool,
    /// Has a review been recorded? Always `false` until Phase 4 builds review,
    /// which is why an accepted policy that requires one holds readiness open.
    pub review_recorded: bool,
    /// The appended waivers, whatever the caller holds.
    pub waivers: &'a [ObligationWaiver],
}

/// The entry in the accepted check profile that records a review requirement.
/// A documentation-only Change records **this**, rather than a pass being
/// invented for it (`B7.7`).
pub const REVIEW_POLICY_ENTRIES: &[&str] =
    &["policy:review-required", "policy:review-instead-of-tests"];

/// Does the accepted check profile record a review requirement?
pub fn policy_requires_review(check_profile: &[String]) -> bool {
    check_profile
        .iter()
        .any(|entry| REVIEW_POLICY_ENTRIES.contains(&entry.trim()))
}

/// **`B7.19`, `B7.21`.** How the accepted check profile records that this
/// Change HAS an exception path, and whose authority it rests on.
///
/// `policy:exceptions-allowed-by:<authority>`. Nothing else is one. A Change
/// whose accepted profile carries no such entry has no exception path at all,
/// and the two write routes refuse rather than inventing an authority: a
/// waiver with an authority the tool made up is the tool deciding, which is
/// the whole of what `B7.19` forbids.
pub const EXCEPTION_POLICY_PREFIX: &str = "policy:exceptions-allowed-by:";

/// The authority the accepted profile configures, or `None` when it configures
/// no exception path.
pub fn exception_authority(check_profile: &[String]) -> Option<String> {
    check_profile.iter().find_map(|entry| {
        entry
            .trim()
            .strip_prefix(EXCEPTION_POLICY_PREFIX)
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(str::to_string)
    })
}

/// The copy slug for the refusal a Change with no configured exception path
/// gets. Looked up like every other sentence; never built at the call site.
pub const NO_EXCEPTION_PATH_SLUG: &str = "readiness_no_exception_path";

/// **The one function both doors call** (`B7.18`-`B7.21`).
///
/// The HTTP route and `thruline change waive` go through here, so an exception
/// written from a terminal and one written from the screen are the same record
/// with the same refusals behind them. It:
///
/// 1. reads the authority off the **accepted** profile, and refuses outright
///    when there is none — the authority is never taken from the request;
/// 2. builds the record and [`ObligationWaiver::validate`]s it, which is where the
///    three non-waivable ids are refused;
/// 3. appends it. Nothing is edited, and an obligation set aside twice is two
///    rows with the later one standing.
pub fn waive(
    ledger: &Ledger,
    change_id: &str,
    obligation_id: &str,
    kind: WaiverKind,
    actor: &str,
    rationale: &str,
    at: &str,
) -> Result<ObligationWaiver> {
    let profile = crate::change_propose::accepted_check_profile(ledger, change_id)?;
    let Some(authority) = exception_authority(&profile) else {
        return Err(CoreError::Config(
            crate::voice::readiness_line(NO_EXCEPTION_PATH_SLUG)
                .map(|l| l.plain.to_string())
                .unwrap_or_else(|| NO_EXCEPTION_PATH_SLUG.to_string()),
        ));
    };
    let record = ObligationWaiver {
        id: format!("cx_{}", ulid::Ulid::generate()),
        obligation_id: obligation_id.trim().to_string(),
        kind,
        actor: actor.trim().to_string(),
        authority,
        rationale: rationale.trim().to_string(),
        at: at.to_string(),
    };
    record.validate()?;
    append_waiver(ledger, change_id, &record)?;
    Ok(record)
}

/// Append one waiver. The only writer of `change_obligation_waivers`.
pub fn append_waiver(ledger: &Ledger, change_id: &str, record: &ObligationWaiver) -> Result<()> {
    record.validate()?;
    ledger.conn().execute(
        "INSERT INTO change_obligation_waivers(id, change_id, obligation_id, kind,
                                               actor, authority, rationale, recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        rusqlite::params![
            record.id,
            change_id,
            record.obligation_id,
            record.kind.as_str(),
            record.actor,
            record.authority,
            record.rationale,
            record.at,
        ],
    )?;
    Ok(())
}

/// Every waiver recorded for one Change, oldest first.
pub fn waivers(ledger: &Ledger, change_id: &str) -> Result<Vec<ObligationWaiver>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, obligation_id, kind, actor, authority, rationale, recorded_at
           FROM change_obligation_waivers WHERE change_id=?1 ORDER BY recorded_at, rowid",
    )?;
    let mapped = statement.query_map(rusqlite::params![change_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in mapped {
        let (id, obligation_id, kind, actor, authority, rationale, at) = row?;
        let kind = match kind.as_str() {
            "not_applicable" => WaiverKind::NotApplicable,
            _ => WaiverKind::Waived,
        };
        out.push(ObligationWaiver {
            id,
            obligation_id,
            kind,
            actor,
            authority,
            rationale,
            at,
        });
    }
    Ok(out)
}

/// **The computation.** Pure, total, deterministic.
///
/// The doc comment at the top of this module is the specification; this
/// function is the only implementation of it.
pub fn compute(input: &ReadinessInput<'_>) -> Readiness {
    let mut obligations: Vec<Obligation> = Vec::new();

    // 2. Candidate identity, first, and non-waivable.
    obligations.push(Obligation {
        id: "candidate_identity".into(),
        kind: ObligationKind::CandidateIdentity,
        criterion_index: None,
        text: String::new(),
        outcome: if input.has_examined_inputs {
            Outcome::Satisfied
        } else {
            Outcome::NeedsEvidence
        },
        outcome_slug: if input.has_examined_inputs {
            Outcome::Satisfied.as_str()
        } else {
            Outcome::NeedsEvidence.as_str()
        },
        unsatisfied: if input.has_examined_inputs {
            None
        } else {
            Some(Unsatisfied::InputsMissing)
        },
        unsatisfied_slug: if input.has_examined_inputs {
            None
        } else {
            Some(Unsatisfied::InputsMissing.slug())
        },
        evidence_name: None,
        linked_by: None,
        waiver: None,
        waivable: false,
    });

    // RUN-LINK-TRUTH: asked once, over the whole Change, because the run it
    // looks for is by definition linked to no line.
    let unlinked_run = unlinked_completed_run(input.coverage).is_some();

    // 3. One obligation per accepted line, in accepted order.
    for row in &input.coverage.criteria {
        let satisfying = row.items.iter().find(|item| admissible(item));
        let (outcome, why, name, linked_by) = match satisfying {
            Some(item) => (
                Outcome::Satisfied,
                None,
                Some(item.name.clone()),
                linker_of(item),
            ),
            None => (
                Outcome::NeedsEvidence,
                Some(why_not(row, unlinked_run)),
                None,
                None,
            ),
        };
        let id = format!("criterion:{}", row.index);
        let waiver = find_waiver(input.waivers, &id);
        let (outcome, why) = match (&waiver, outcome) {
            // A waiver only ever speaks about an obligation that is not
            // satisfied. Recording one over a satisfied line would let a
            // waiver hide a real result, so the evidence wins.
            (Some(d), Outcome::NeedsEvidence) => (d.kind.outcome(), why),
            _ => (outcome, why),
        };
        obligations.push(Obligation {
            id,
            kind: ObligationKind::Criterion,
            criterion_index: Some(row.index),
            text: row.text.clone(),
            outcome,
            outcome_slug: outcome.as_str(),
            unsatisfied: why,
            unsatisfied_slug: why.map(|w| w.slug()),
            evidence_name: name,
            linked_by,
            waiver,
            waivable: true,
        });
    }

    // 4. The review obligation — only when the accepted policy records one.
    if input.review_required {
        let waiver = find_waiver(input.waivers, "review");
        let outcome = if input.review_recorded {
            Outcome::Satisfied
        } else {
            match &waiver {
                Some(d) => d.kind.outcome(),
                None => Outcome::NeedsEvidence,
            }
        };
        obligations.push(Obligation {
            id: "review".into(),
            kind: ObligationKind::Review,
            criterion_index: None,
            text: String::new(),
            outcome,
            outcome_slug: outcome.as_str(),
            unsatisfied: if outcome == Outcome::NeedsEvidence {
                Some(Unsatisfied::ReviewMissing)
            } else {
                None
            },
            unsatisfied_slug: if outcome == Outcome::NeedsEvidence {
                Some(Unsatisfied::ReviewMissing.slug())
            } else {
                None
            },
            evidence_name: None,
            linked_by: None,
            waiver,
            waivable: true,
        });
    }

    // 1. The empty set. Asked LAST so the obligations are still listed, and
    // answered FIRST so nothing else can overrule it.
    let nothing_accepted =
        input.coverage.scope_version.is_none() || input.coverage.criteria.is_empty();

    let missing: Vec<String> = obligations
        .iter()
        .filter(|o| o.outcome == Outcome::NeedsEvidence)
        .map(|o| o.id.clone())
        .collect();
    let waived: Vec<String> = obligations
        .iter()
        .filter(|o| o.outcome == Outcome::Waived)
        .map(|o| o.id.clone())
        .collect();
    let not_applicable: Vec<String> = obligations
        .iter()
        .filter(|o| o.outcome == Outcome::NotApplicable)
        .map(|o| o.id.clone())
        .collect();

    let ready = !nothing_accepted
        && input.target_version.is_some()
        && obligations.iter().all(|o| o.outcome.clears());
    /* **`B7.22`.** Ready, and something on it was set aside rather than
     * checked. It is a SEPARATE field from `ready` and a separate headline,
     * because "Approved" and "Approved with exceptions" are two different
     * things a person is being asked to do, and one word for both is how the
     * second becomes invisible. */
    let with_exceptions = ready && !(waived.is_empty() && not_applicable.is_empty());

    let rests_on: Vec<RestsOn> = if ready {
        obligations
            .iter()
            .filter(|o| o.outcome.is_evidence() && o.kind == ObligationKind::Criterion)
            .map(|o| RestsOn {
                obligation_id: o.id.clone(),
                text: o.text.clone(),
                evidence_name: o.evidence_name.clone(),
                linked_by: o.linked_by.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    let headline_slug = if nothing_accepted {
        "readiness_nothing_accepted"
    } else if with_exceptions {
        "readiness_ready_with_exceptions"
    } else if ready {
        "readiness_ready"
    } else if missing.len() == 1 {
        "readiness_needs_evidence_one"
    } else {
        "readiness_needs_evidence"
    };

    Readiness {
        schema_version: READINESS_SCHEMA_VERSION,
        change_id: input.change_id.to_string(),
        ready,
        with_exceptions,
        scope_version: input.coverage.scope_version,
        target_version: input.target_version.clone(),
        headline_slug,
        obligations,
        rests_on,
        missing,
        waived,
        not_applicable,
    }
}

/// The six conditions of the module doc, and nothing else.
///
/// The applicability condition is asked HERE, beside the observed result, so a
/// line whose evidence was never assessed is kept out of Ready in the same
/// place and by the same mechanism as Couldn't check (R-2026-09-28-25).
fn admissible(item: &crate::change_evidence::EvidenceItemView) -> bool {
    settles_on_this_version(item) && item.fresh == Some(true)
}

/// Every condition of [`admissible`] except whether its applicability has been
/// assessed. Private: it exists so [`admissible`] and [`never_assessed`] share
/// one reading of the other five and cannot drift.
fn settles_on_this_version(item: &crate::change_evidence::EvidenceItemView) -> bool {
    if !item.bears {
        return false;
    }
    // A completion claim is not an examination and never satisfies a line.
    if item.kind == EvidenceKind::Finding {
        return false;
    }
    if item.observed != Observed::Proven {
        return false;
    }
    if item.applies != Some(true) {
        return false;
    }
    // A *recorded* execution state that is not `completed` never satisfies
    // anything (T-3-4). The absence of a row is the absence of a row: runs
    // captured before the supervisor existed have none, and they are judged on
    // the four conditions above.
    matches!(
        item.execution,
        ExecutionState::Completed | ExecutionState::NotRun
    )
}

/// **R-2026-09-28-25 — the one rule for "never assessed".** True when nothing
/// satisfies this line, and at least one item linked to it would satisfy it
/// but for one missing fact: no applicability assessment exists for it. That
/// is exactly the line whose one missing act is an assessment.
///
/// [`compute`] names such a line [`Unsatisfied::NeverAssessed`], and every
/// surface (the Change screen, the packet page, the exported file) reads that
/// served reason rather than working one out of the items.
pub fn never_assessed(row: &crate::change_evidence::CriterionCoverage) -> bool {
    !row.items.iter().any(admissible)
        && row
            .items
            .iter()
            .any(|i| settles_on_this_version(i) && i.fresh.is_none())
}

/// **RUN-LINK-TRUTH — the one rule for "a test ran and isn't linked".** A
/// completed check run for this Change that bears, observed Proven, and that
/// no accepted line has linked. The same typed fields the Change screen's
/// `unlinkedCompletedRun` reads (`coverage-view.js`), and a UI test pins that
/// the two name the same lines.
///
/// It only ever changes which REASON an unsatisfied line gives. It links
/// nothing, and nothing it finds counts for any line: only a named person's
/// link (write address #4) does that.
pub fn unlinked_completed_run(
    coverage: &CoverageView,
) -> Option<&crate::change_evidence::CandidateView> {
    let linked: std::collections::BTreeSet<&str> = coverage
        .criteria
        .iter()
        .flat_map(|c| c.items.iter().map(|i| i.evidence_id.as_str()))
        .collect();
    coverage.candidates.iter().find(|c| {
        c.kind == EvidenceKind::CheckRun
            && c.execution == ExecutionState::Completed
            && c.bears
            && c.observed == Observed::Proven
            && !c.evidence_id.is_empty()
            && !linked.contains(c.evidence_id.as_str())
    })
}

/// The person whose link makes this item count, when it counts through one.
fn linker_of(item: &crate::change_evidence::EvidenceItemView) -> Option<String> {
    if item.basis != crate::change_evidence::LinkBasis::Manual {
        return None;
    }
    item.actor
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
}

/// Why nothing admissible was found for one line, from the coverage state the
/// engine already derived plus the items themselves. One answer, chosen in the
/// order a person needs to hear it.
///
/// `unlinked_run` is [`unlinked_completed_run`]'s answer for the whole Change.
fn why_not(row: &crate::change_evidence::CriterionCoverage, unlinked_run: bool) -> Unsatisfied {
    // R-2026-09-28-25: asked before anything else. Something on this version
    // would settle the line, and the only thing missing is the assessment of
    // whether it still applies — so that is what a person is told, with the
    // act that supplies it, rather than a reason about some other item.
    if never_assessed(row) {
        // NEVER-ASSESSED-NOT-READY-R2: "Run `thruline change prepare` and it
        // will be." is said only when that is what will happen. When no item
        // that would settle the line can be assessed by a prepare, the line
        // says so instead, and names what would settle it.
        let prepare_cannot = row
            .items
            .iter()
            .filter(|i| settles_on_this_version(i) && i.fresh.is_none())
            .all(|i| i.assessable == Some(false));
        return if prepare_cannot {
            Unsatisfied::CouldNotAssess
        } else {
            Unsatisfied::NeverAssessed
        };
    }
    // **T-3-5 verifier finding 2, half two — and this is the single place the
    // order is decided.** Execution state is asked FIRST, before the coverage
    // state is consulted at all.
    //
    // A run that was stopped settles nothing, so it does not bear, so the
    // coverage state derived from what the items *observed* is `RanNothing` —
    // "something ran, and it settled nothing about this". That sentence is
    // true and it is the wrong one: it hides the fact a person most needs,
    // which is that a check was started and stopped, by someone, before it
    // could answer. What became of the RUN outranks what its output managed to
    // say, always, so the question is asked in that order rather than nested
    // inside the `Evidence` arm where only a run that still bore on the line
    // could reach it.
    //
    // `None` and `AbsentTool` are unaffected by construction: neither has any
    // items, so there is no execution to consult.
    if row.items.iter().any(|i| {
        !matches!(
            i.execution,
            ExecutionState::Completed | ExecutionState::NotRun
        )
    }) {
        return Unsatisfied::Stopped;
    }
    match row.state {
        // RUN-LINK-TRUTH: nothing is linked to this line, and a passing run
        // for this Change is linked to no line. "No test ran for this." would
        // be false; the line says the run is there and is not linked.
        CoverageState::None if unlinked_run => return Unsatisfied::UnlinkedRun,
        CoverageState::None => return Unsatisfied::NothingLooked,
        CoverageState::AbsentTool => return Unsatisfied::ToolAbsent,
        CoverageState::RanNothing => return Unsatisfied::SettledNothing,
        CoverageState::OtherVersion => return Unsatisfied::OtherVersion,
        CoverageState::Evidence => {}
    }
    let bearing: Vec<_> = row.items.iter().filter(|i| i.bears).collect();
    if bearing.is_empty() {
        return Unsatisfied::NothingLooked;
    }
    // (The execution question used to be asked here, over the BEARING items
    // only. It is asked above now, over all of them, which is strictly wider:
    // every case this arm caught it still catches.)
    if bearing.iter().any(|i| i.fresh == Some(false)) {
        return Unsatisfied::Stale;
    }
    if bearing.iter().any(|i| i.observed == Observed::NotProven) {
        return Unsatisfied::NotProven;
    }
    if bearing
        .iter()
        .any(|i| i.observed == Observed::Proven && i.applies != Some(true))
    {
        return Unsatisfied::OtherVersion;
    }
    if bearing.iter().any(|i| i.observed == Observed::Loose) {
        return Unsatisfied::SettledNothing;
    }
    Unsatisfied::Unknown
}

fn find_waiver(all: &[ObligationWaiver], id: &str) -> Option<ObligationWaiver> {
    all.iter()
        .rfind(|d| d.obligation_id.trim() == id && d.validate().is_ok())
        .cloned()
}

/// Read the records and compute. Decides nothing itself.
///
/// The caller passes the [`CoverageView`] it already built, so the derivation
/// is not run twice and the served readiness is about the coverage a person is
/// looking at rather than a second reading of the store.
pub fn readiness(ledger: &Ledger, change_id: &str, coverage: &CoverageView) -> Result<Readiness> {
    let recorded = waivers(ledger, change_id)?;
    let intended = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?;
    let target_version = intended.as_ref().and_then(|i| {
        i.snapshot_id
            .value()
            .or_else(|| i.base_revision.value())
            .map(|v| v.to_string())
    });
    let profile = crate::change_propose::accepted_check_profile(ledger, change_id)?;
    Ok(compute(&ReadinessInput {
        change_id,
        coverage,
        target_version,
        has_examined_inputs: intended.is_some(),
        review_required: policy_requires_review(&profile),
        review_recorded: false,
        waivers: &recorded,
    }))
}
