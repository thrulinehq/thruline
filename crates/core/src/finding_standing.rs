//! T-4-2 — where each thing a review found stands (`B9.10`, `B9.11`, `B9.13`,
//! `B9.14`, `Q9`, `Q10`).
//!
//! A review (T-4-1, [`crate::change_review`]) raises findings. This module
//! records, one appended row at a time, what became of each of them. It is the
//! only writer of `change_review_finding_standings`, and [`record`] is the one
//! function both doors call: `POST /api/changes/<id>/findings/<finding>/standing`
//! and `thruline change finding`.
//!
//! ## Seven standings, and none of them is a verdict
//!
//! `open` · `under_repair` · `awaiting_verification` · `fixed_with_evidence` ·
//! `disputed` · `informational` · `waived`. These are the technical names; a
//! screen shows the copy file's plain labels. They share no word with the five
//! state words or the claim-verdict vocabulary (`B9.11`), and
//! `hq-copy.test.js` holds that mechanically.
//!
//! ## The rules, held here rather than on a screen
//!
//! 1. **Append-only.** Every change of standing is a new row naming the
//!    finding, from and to, the actor, the authority, the reason, the time and
//!    the evidence. The standing is the last row; a finding with no row is
//!    `open` with no decision recorded. Triggers refuse UPDATE and DELETE, and
//!    a row that does not start where the previous one ended.
//! 2. **`fixed_with_evidence` needs evidence** ([`check_evidence`]): a record
//!    bound to the same Change, examined on a version LATER than the one the
//!    finding was raised on, that applies to the version in front of you now,
//!    and that bears on the finding — either a later review of the current
//!    version that no longer raises anything at that place, or a completed,
//!    passing check run of this Change whose scope covers the finding's file.
//!    And the re-check must have looked at CHANGED code
//!    ([`recheck_saw_changed_code`], ADV-review-truth-1): a new version id on
//!    the same code, or a review that read the same diff or an empty one, is
//!    refused as `evidence_same_code`.
//!    Anything less is refused with a code that says which part was missing.
//! 3. **A closed or resolved comment is not a fix** (`B9.13`, `Q9`). A
//!    "resolved" mark — typed by a person, or a labelled synthetic import (the
//!    plain-diff review route produces no comment threads, so there is no real
//!    provider thread to import) — reaches `awaiting_verification` and nothing
//!    further.
//! 4. **Setting a finding aside needs a reason and an authority** (`B9.14`,
//!    `Q10`): `disputed`, `waived` and `informational` each need a non-empty
//!    reason, a named actor and the authority they decide under. The refusal
//!    names every part that is missing. Text nobody can read is empty
//!    ([`visible_text`]): a zero-width space, an interlinear annotation
//!    character or an accent over nothing is not a reason. No typed part
//!    is longer than [`MAX_PART_CHARS`] characters, on either door.
//! 5. **Set aside is never fixed.** [`Group`] keeps `disputed` and `waived`
//!    apart from `fixed_with_evidence`, and `counts_as_fixed` is true only for
//!    a fix whose evidence still applies to the version in front of you.
//!
//! No `unwrap`, no `expect`, no network. Free text is masked before it is
//! served.

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::change::mask_text;
use crate::change_review::{ReviewFinding, ReviewState, ReviewView};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

pub use crate::ledger::{FINDING_STANDING_MIGRATION, FINDING_STANDING_TABLE};

// The Unicode 16 table `is_legible` reads (ADV-review-input-4).
mod legible;

/// The served shape's version. Bumped when a field is added or renamed.
pub const FINDINGS_VIEW_SCHEMA_VERSION: u32 = 2;

// ---------------------------------------------------------------------------
// The vocabulary
// ---------------------------------------------------------------------------

/// Where one finding stands. Closed set of seven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    Open,
    UnderRepair,
    AwaitingVerification,
    FixedWithEvidence,
    Disputed,
    Informational,
    Waived,
}

impl Standing {
    pub const ALL: [Standing; 7] = [
        Standing::Open,
        Standing::UnderRepair,
        Standing::AwaitingVerification,
        Standing::FixedWithEvidence,
        Standing::Disputed,
        Standing::Informational,
        Standing::Waived,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::UnderRepair => "under_repair",
            Self::AwaitingVerification => "awaiting_verification",
            Self::FixedWithEvidence => "fixed_with_evidence",
            Self::Disputed => "disputed",
            Self::Informational => "informational",
            Self::Waived => "waived",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == value)
            .ok_or_else(|| CoreError::Config(format!("unknown finding standing {value:?}")))
    }

    /// The three standings that set a finding aside without a fix. Each needs
    /// a reason and an authority (`B9.14`, `Q10`).
    pub fn sets_aside(self) -> bool {
        matches!(self, Self::Disputed | Self::Informational | Self::Waived)
    }

    /// The plain label, as the copy file (`HQ_COPY.change.review.standing.labels`)
    /// holds it. The terminal door prints these; `finding_standing_copy.rs`
    /// asserts they equal the copy file's golden, so the two doors cannot say
    /// two different things.
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Still open",
            Self::UnderRepair => "Being repaired",
            Self::AwaitingVerification => "Waiting for a re-check",
            Self::FixedWithEvidence => "Fixed",
            Self::Disputed => "Disputed",
            Self::Informational => "For information only",
            Self::Waived => "Waived",
        }
    }
}

/// How a change of standing arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// A person chose a standing.
    Person,
    /// A person typed that the comment is resolved or closed. Not a fix.
    ResolvedMark,
    /// A "resolved" signal from a synthetic import, labelled as one. Nothing
    /// in this product imports real provider comment threads.
    SyntheticResolvedImport,
}

impl Via {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::ResolvedMark => "resolved_mark",
            Self::SyntheticResolvedImport => "synthetic_resolved_import",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "person" => Ok(Self::Person),
            "resolved_mark" => Ok(Self::ResolvedMark),
            "synthetic_resolved_import" => Ok(Self::SyntheticResolvedImport),
            other => Err(CoreError::Config(format!(
                "unknown standing source {other:?}"
            ))),
        }
    }

    pub fn is_resolved_mark(self) -> bool {
        !matches!(self, Self::Person)
    }
}

/// What kind of record shows a fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// A later review round, on the current version, that no longer raises it.
    Review,
    /// A completed, passing check run of this Change that covers its file.
    CheckRun,
}

impl EvidenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::CheckRun => "check_run",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "review" => Ok(Self::Review),
            "check_run" => Ok(Self::CheckRun),
            other => Err(CoreError::Config(format!(
                "unknown evidence kind {other:?}"
            ))),
        }
    }
}

/// A reference to one record offered as evidence of a fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub kind: EvidenceKind,
    pub id: String,
}

/// Which list a finding belongs in on a screen. Five groups, and the point is
/// that `SetAside` is never `Fixed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// `open` or `under_repair`.
    StillWrong,
    /// `awaiting_verification`, or a fix whose evidence no longer applies.
    AwaitingRecheck,
    /// `fixed_with_evidence`, and the evidence applies to this version.
    Fixed,
    /// `disputed` or `waived`: a person decided, nothing showed a fix.
    SetAside,
    /// `informational`: not every finding needs a code change.
    Informational,
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// One finding's stable key: `<review id>:<ordinal>`.
pub fn finding_key(review_id: &str, ordinal: u32) -> String {
    format!("{review_id}:{ordinal}")
}

/// The inverse of [`finding_key`], or `None` for anything else.
pub fn parse_finding_key(key: &str) -> Option<(String, u32)> {
    let (review, ordinal) = key.rsplit_once(':')?;
    if review.trim().is_empty() || review.contains('/') {
        return None;
    }
    let ordinal = ordinal.parse::<u32>().ok()?;
    Some((review.to_string(), ordinal))
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

/// The most characters a person may type into one part of an act — the name,
/// the authority, the reason — on either door (ADV-review-input-2). Counted as
/// characters (Unicode scalar values), not bytes, after [`visible_text`]. The
/// screen's route sizes its body cap from this number, `review-model.js`
/// holds the same number as `MAX_PART_CHARS`, and the copy file's `too_long`
/// sentence states it; `finding_standing_copy.rs` and `review-model.test.js`
/// hold the three equal.
pub const MAX_PART_CHARS: usize = 2000;

/// Why an act was refused. `code` is a closed set of copy slugs
/// (`HQ_COPY.change.review.standing.refused.<code>`); `missing` names every
/// part a person left out, and `too_long` every part over
/// [`MAX_PART_CHARS`], so the sentence can say which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refusal {
    pub code: &'static str,
    pub missing: Vec<&'static str>,
    /// Served only when a part is over the limit, so every other refusal
    /// keeps the shape it had.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub too_long: Vec<&'static str>,
    /// Where the finding stands now, when that is the reason.
    pub current: Option<Standing>,
}

impl Refusal {
    fn because(code: &'static str) -> Self {
        Self {
            code,
            missing: vec![],
            too_long: vec![],
            current: None,
        }
    }
}

/// Every refusal code this module can return. A screen that has no sentence
/// for one of them fails `review-model.test.js`.
pub const REFUSAL_CODES: &[&str] = &[
    "no_such_finding",
    "finding_not_readable",
    "moved",
    "unchanged",
    "missing",
    "evidence_required",
    "evidence_not_for_this",
    "mark_goes_no_further",
    "mark_needs_an_open_finding",
    "evidence_not_found",
    "evidence_not_finished",
    "evidence_not_current",
    "evidence_not_later",
    "evidence_same_code",
    "evidence_still_raised",
    "evidence_raised_elsewhere",
    "evidence_not_passing",
    "evidence_no_place",
    "evidence_outside_scope",
    "too_long",
];

/// The terminal door's sentence for each refusal code, word for word from
/// `HQ_COPY.change.review.standing.refused` (fenced by `finding_standing_copy.rs`).
pub fn refusal_line(code: &str) -> &'static str {
    match code {
        "no_such_finding" => "This is not one of the things a review of this Change raised.",
        "finding_not_readable" => "The review that raised this no longer matches what was written down, so nothing about it can be settled.",
        "moved" => "Where this stands changed while you were looking. Nothing was saved; look again and choose.",
        "unchanged" => "It already stands there, so nothing was saved.",
        "missing" => "Nothing was saved: some of what this needs is still empty.",
        "evidence_required" => "Nothing was saved: a fix needs something that shows it.",
        "evidence_not_for_this" => "Nothing was saved: only a fix is saved with something that shows it.",
        "mark_goes_no_further" => "Nothing was saved: marking a comment resolved can only say it is waiting for a re-check, never that it was fixed.",
        "mark_needs_an_open_finding" => "Nothing was saved: only something still open can be marked resolved.",
        "evidence_not_found" => "Nothing was saved: that re-check is not part of this Change.",
        "evidence_not_finished" => "Nothing was saved: that re-check did not finish, so it shows nothing.",
        "evidence_not_current" => "Nothing was saved: that re-check is not about the version in front of you.",
        "evidence_not_later" => "Nothing was saved: that re-check looked at the same version this was raised on, so it cannot show a fix.",
        "evidence_same_code" => "Nothing was saved: nothing shows that re-check looked at changed code, so it cannot show a fix.",
        "evidence_still_raised" => "Nothing was saved: that re-check still raises something in the same place.",
        "evidence_raised_elsewhere" => "Nothing was saved: that re-check raises the same thing in another file, so the file may have moved and it cannot show a fix.",
        "evidence_not_passing" => "Nothing was saved: that check did not pass.",
        "evidence_no_place" => "Nothing was saved: this names no file, so no check can be shown to cover it. A later review that no longer raises it can.",
        "evidence_outside_scope" => "Nothing was saved: that check left this file out of what it looked at.",
        "too_long" => "Nothing was saved: something you typed is longer than 2,000 characters.",
        _ => "Nothing was saved: the record could not be written just now.",
    }
}

/// The words for each part a person left out, from
/// `HQ_COPY.change.review.standing.parts`.
pub fn part_words(part: &str) -> &'static str {
    match part {
        "actor" => "your name",
        "reason" => "the reason",
        "authority" => "why you can decide this",
        _ => "something this needs",
    }
}

// ---------------------------------------------------------------------------
// The act
// ---------------------------------------------------------------------------

/// One person's (or one labelled import's) request to change one finding's
/// standing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandingAct {
    /// [`finding_key`].
    pub finding: String,
    pub to: Standing,
    pub via: Via,
    pub actor: String,
    /// The authority the actor decides under ("I own this project", "the
    /// reviewer who raised it"). Required when setting a finding aside.
    pub authority: String,
    pub reason: String,
    pub evidence: Option<EvidenceRef>,
    /// The standing the screen was showing. When given and different from the
    /// record's, the act is refused as `moved` rather than written over it.
    pub seen: Option<Standing>,
    pub at: String,
}

/// What [`record`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Recorded {
        id: String,
        finding: String,
        from: Standing,
        to: Standing,
    },
    Refused(Refusal),
}

/// The finding, as the record holds it, with its review's facts.
struct FindingContext {
    change_id: String,
    review_round: u32,
    /// The version the finding was raised on.
    raised_on: String,
    /// The examined-inputs record the raising review was bound to: the code
    /// the finding was raised on.
    examined_inputs_id: String,
    /// The digest of the diff the raising review read.
    diff_sha256: String,
    path: Option<String>,
    kind: String,
    message: String,
}

/// Everything the evidence rule reads, gathered once.
struct EvidenceWorld {
    reviews: Vec<ReviewView>,
    current_revision: Option<String>,
    candidates: Option<Vec<crate::change_evidence::Candidate>>,
    intended: Option<crate::examined_inputs::ExaminedInputs>,
}

impl EvidenceWorld {
    fn gather(ledger: &Ledger, change_id: &str, masking: &mut MaskRecord) -> Result<Self> {
        let reviews = crate::change_review::read_change_reviews(ledger, change_id, masking)?;
        let (current_revision, _) = crate::change_decision::standing_target(ledger, change_id)?;
        let intended = crate::examined_inputs::standing(
            ledger,
            crate::examined_inputs::SubjectKind::Intended,
            change_id,
        )?;
        Ok(Self {
            reviews,
            current_revision,
            candidates: None,
            intended,
        })
    }

    fn candidates(
        &mut self,
        ledger: &Ledger,
        change_id: &str,
    ) -> Result<&[crate::change_evidence::Candidate]> {
        if self.candidates.is_none() {
            self.candidates = Some(crate::change_evidence::candidates(ledger, change_id)?);
        }
        Ok(self.candidates.as_deref().unwrap_or(&[]))
    }
}

/// Read one finding's context. `None` when there is no such finding on this
/// Change, or when its review is not one whose findings can be acted on.
fn finding_context(
    world: &EvidenceWorld,
    change_id: &str,
    review_id: &str,
    ordinal: u32,
) -> std::result::Result<FindingContext, &'static str> {
    let Some(review) = world.reviews.iter().find(|r| r.id == review_id) else {
        return Err("no_such_finding");
    };
    // `read_change_reviews` serves findings only for an intact, complete
    // review: a finding of an altered or unfinished review is not something a
    // person can be asked to settle.
    if review.state != ReviewState::Complete
        || review.integrity != crate::change_review::Integrity::Intact
    {
        return Err("finding_not_readable");
    }
    let Some(finding) = review.findings.get(ordinal as usize) else {
        return Err("no_such_finding");
    };
    Ok(FindingContext {
        change_id: change_id.to_string(),
        review_round: review.review_round,
        raised_on: review.target_revision.clone(),
        examined_inputs_id: review.examined_inputs_id.clone(),
        diff_sha256: review.diff_sha256.clone(),
        path: finding.path.clone(),
        kind: finding.kind.clone(),
        message: finding.message.clone(),
    })
}

/// The finding's standing now: the last row, or `open` when there is none.
fn current_standing(ledger: &Ledger, review_id: &str, ordinal: u32) -> Result<(Standing, bool)> {
    let row: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT to_standing FROM change_review_finding_standings
              WHERE review_id=?1 AND ordinal=?2 ORDER BY rowid DESC LIMIT 1",
            params![review_id, ordinal as i64],
            |row| row.get(0),
        )
        .optional()?;
    match row {
        Some(s) => Ok((Standing::parse(&s)?, true)),
        None => Ok((Standing::Open, false)),
    }
}

/// Does one later review still raise something at the finding's place?
///
/// Deliberately conservative: a review is only evidence of a fix when it
/// raises NOTHING in the same file (for a finding that names a file), or
/// nothing of the same kind or with the same words (for one that does not).
/// A reviewer's wording and line numbers drift between rounds; the file does
/// not, and erring towards "still raised" is erring towards not calling a
/// finding fixed.
fn still_raised(context: &FindingContext, later: &[ReviewFinding]) -> bool {
    let norm = |s: &str| s.trim().to_lowercase();
    later.iter().any(|f| match (&context.path, &f.path) {
        (Some(mine), Some(theirs)) => norm(mine) == norm(theirs),
        (Some(_), None) => false,
        (None, _) => {
            norm(&f.kind) == norm(&context.kind)
                || (!context.message.trim().is_empty()
                    && norm(&f.message) == norm(&context.message))
        }
    })
}

/// Does one later review raise the same thing somewhere else
/// (FINDING-RENAME-LAUNDERING)?
///
/// [`still_raised`] matches a finding that names a file by its file alone, so
/// a file renamed between rounds takes the finding out of its sight: the
/// original path is absent from the re-check even though the defect is not.
/// Nothing in the record maps one version's paths onto the next's, so the
/// original path being absent is never taken to show the finding is gone
/// while the re-check raises something of the same kind, or with the same
/// words, in another file or in none.
fn raised_elsewhere(context: &FindingContext, later: &[ReviewFinding]) -> bool {
    let norm = |s: &str| s.trim().to_lowercase();
    if context.path.is_none() {
        return false;
    }
    later.iter().any(|f| {
        (!context.kind.trim().is_empty() && norm(&f.kind) == norm(&context.kind))
            || (!context.message.trim().is_empty() && norm(&f.message) == norm(&context.message))
    })
}

/// What a piece of evidence showed, once it passed [`check_evidence`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceView {
    pub kind: EvidenceKind,
    pub id: String,
    /// The version the evidence was examined on.
    pub revision: String,
    /// The review round, when the evidence is a review.
    pub review_round: Option<u32>,
    /// The record's own name for it (a reviewer, or a check's command), masked.
    pub name: String,
}

/// **Rule 2, in one place.** Is this record evidence that this finding was
/// fixed, on the version in front of you now?
///
/// Returns the evidence as the record holds it, or the refusal code naming
/// which condition failed. What a screen OFFERS as evidence and what [`record`]
/// ACCEPTS is this function followed by [`recheck_saw_changed_code`]
/// ([`acceptable_evidence`]); read time asks this one again to say whether a
/// recorded fix still applies.
fn check_evidence(
    ledger: &Ledger,
    world: &mut EvidenceWorld,
    context: &FindingContext,
    evidence: &EvidenceRef,
) -> Result<std::result::Result<EvidenceView, &'static str>> {
    // A fix can only be shown on a version later than the one the finding was
    // raised on: if the code has not moved, nothing about it was repaired.
    let Some(current) = world.current_revision.clone() else {
        return Ok(Err("evidence_not_current"));
    };
    match evidence.kind {
        EvidenceKind::Review => {
            let Some(later) = world.reviews.iter().find(|r| r.id == evidence.id) else {
                return Ok(Err("evidence_not_found"));
            };
            if later.state != ReviewState::Complete
                || later.integrity != crate::change_review::Integrity::Intact
            {
                return Ok(Err("evidence_not_finished"));
            }
            if !later.applies_to_current_version {
                return Ok(Err("evidence_not_current"));
            }
            if later.target_revision == context.raised_on
                || later.review_round <= context.review_round
            {
                return Ok(Err("evidence_not_later"));
            }
            if still_raised(context, &later.findings) {
                return Ok(Err("evidence_still_raised"));
            }
            if raised_elsewhere(context, &later.findings) {
                return Ok(Err("evidence_raised_elsewhere"));
            }
            Ok(Ok(EvidenceView {
                kind: EvidenceKind::Review,
                id: later.id.clone(),
                revision: later.target_revision.clone(),
                review_round: Some(later.review_round),
                name: later.reviewer_identity.clone(),
            }))
        }
        EvidenceKind::CheckRun => {
            let change_id = context.change_id.clone();
            let candidate = world
                .candidates(ledger, &change_id)?
                .iter()
                .find(|c| {
                    c.kind == crate::change_evidence::EvidenceKind::CheckRun && c.id == evidence.id
                })
                .cloned();
            let Some(candidate) = candidate else {
                return Ok(Err("evidence_not_found"));
            };
            use crate::run_execution::ExecutionState;
            if !matches!(
                candidate.execution,
                ExecutionState::Completed | ExecutionState::NotRun
            ) {
                return Ok(Err("evidence_not_finished"));
            }
            if candidate.observed != crate::change_evidence::Observed::Proven || !candidate.bears()
            {
                return Ok(Err("evidence_not_passing"));
            }
            // "Is this about the version in front of you" has ONE rule in this
            // workspace (`change_evidence::applicability_of`, pinned by the
            // census in `examined_inputs_predicate.rs`). Asked there, never
            // re-decided here.
            let applies = crate::change_evidence::applicability_of(
                ledger,
                world.intended.as_ref(),
                crate::change_evidence::EvidenceKind::CheckRun,
                &candidate.id,
            )?;
            if !applies.is_some_and(|a| a.applies()) {
                return Ok(Err("evidence_not_current"));
            }
            if current == context.raised_on {
                return Ok(Err("evidence_not_later"));
            }
            // A check can only be said to cover a place the finding names.
            let Some(path) = context
                .path
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
            else {
                return Ok(Err("evidence_no_place"));
            };
            let binding = match crate::evidence_binding::load(ledger, &candidate.id) {
                Ok(Some(binding)) => binding,
                Ok(None) | Err(_) => return Ok(Err("evidence_not_found")),
            };
            if binding
                .binding
                .wsid_scope
                .exclusions
                .iter()
                .any(|excluded| !excluded.is_empty() && path.starts_with(excluded.as_str()))
            {
                return Ok(Err("evidence_outside_scope"));
            }
            Ok(Ok(EvidenceView {
                kind: EvidenceKind::CheckRun,
                id: candidate.id.clone(),
                revision: current,
                review_round: None,
                name: candidate.name.clone(),
            }))
        }
    }
}

/// **Rule 2's second half (ADV-review-truth-1).** Did the re-check look at code
/// that changed after the finding was raised, and re-run on it?
///
/// [`check_evidence`]'s "later" compares version ids and review rounds, and a
/// `change prepare` with nothing changed mints a new version id on the same
/// code. So this asks the code itself, through the one predicate
/// ([`crate::examined_inputs::observation_applies`]): the re-check's examined
/// inputs set against the inputs the finding's review was bound to. The code
/// moved only when the answer is another commit (`inputs_other_revision`) or
/// other uncommitted changes in the same folder (`inputs_other_edits`).
/// Everything else — the same code, a folder-state-only difference, a second
/// checkout, an unknown side, a missing record — shows no change, and erring
/// that way is erring towards not calling a finding fixed.
///
/// A review is a re-run on the changed code only when it also read a diff other
/// than the one the finding was raised on, and not an empty one: a reviewer
/// that read the same bytes, or none, cannot have seen a repair.
///
/// A refusal of the transition, nothing more: [`record`] asks it and the offer
/// on a screen asks it (so nothing is offered that would be refused). The
/// read-time question about a fix ALREADY recorded stays [`check_evidence`]'s
/// alone, so no recorded standing is derived differently from before.
fn recheck_saw_changed_code(
    ledger: &Ledger,
    world: &EvidenceWorld,
    context: &FindingContext,
    evidence: &EvidenceRef,
) -> Result<bool> {
    use crate::examined_inputs::{by_id, standing, Applies, SubjectKind};
    let Some(raised) = by_id(ledger, &context.examined_inputs_id)? else {
        return Ok(false);
    };
    let recheck = match evidence.kind {
        EvidenceKind::Review => {
            let Some(later) = world.reviews.iter().find(|r| r.id == evidence.id) else {
                return Ok(false);
            };
            let empty = crate::chain::sha256_hex(b"");
            if later.diff_sha256 == context.diff_sha256 || later.diff_sha256 == empty {
                return Ok(false);
            }
            by_id(ledger, &later.examined_inputs_id)?
        }
        EvidenceKind::CheckRun => standing(ledger, SubjectKind::CheckRun, &evidence.id)?,
    };
    let Some(recheck) = recheck else {
        return Ok(false);
    };
    let moved = crate::examined_inputs::observation_applies(&recheck, &raised);
    Ok(matches!(
        moved,
        Applies::No {
            reason_slug: "inputs_other_revision" | "inputs_other_edits"
        }
    ))
}

/// What [`record`] accepts and a screen offers as evidence of a fix:
/// [`check_evidence`], and then [`recheck_saw_changed_code`].
fn acceptable_evidence(
    ledger: &Ledger,
    world: &mut EvidenceWorld,
    context: &FindingContext,
    evidence: &EvidenceRef,
) -> Result<std::result::Result<EvidenceView, &'static str>> {
    let view = match check_evidence(ledger, world, context, evidence)? {
        Ok(view) => view,
        Err(code) => return Ok(Err(code)),
    };
    if !recheck_saw_changed_code(ledger, world, context, evidence)? {
        return Ok(Err("evidence_same_code"));
    }
    Ok(Ok(view))
}

/// Whether a person could not see this character on a screen or in a terminal
/// (ADV-review-input-1): `White_Space`, the control characters (`Cc`), every
/// `Default_Ignorable_Code_Point` of Unicode 16 (zero-width spaces and
/// joiners, the byte-order mark, the soft hyphen, the bidi controls, the
/// Hangul fillers, the variation selectors, the tag characters), the blank
/// braille pattern U+2800, which draws nothing, and the object replacement
/// character U+FFFC, which Chrome draws with no ink and no width — the one
/// letter, number, punctuation mark or symbol that a headless Chrome sweep of
/// every one of them, in the Change screen's fonts, found blank
/// (ADV-review-input-4). `review-model.js`'s
/// `isInvisible` is the same set written with Unicode property escapes, and
/// `review-model.test.js` walks every code point to hold the two equal, so
/// the ranges below are read by that test and must stay written as hex.
pub fn is_invisible(c: char) -> bool {
    c.is_whitespace()
        || c.is_control()
        || matches!(
            c as u32,
            0x00AD
                | 0x034F
                | 0x061C
                | 0x115F..=0x1160
                | 0x17B4..=0x17B5
                | 0x180B..=0x180F
                | 0x200B..=0x200F
                | 0x202A..=0x202E
                | 0x2060..=0x206F
                | 0x2800
                | 0x3164
                | 0xFE00..=0xFE0F
                | 0xFEFF
                | 0xFFA0
                | 0xFFF0..=0xFFF8
                | 0xFFFC
                | 0x1BCA0..=0x1BCA3
                | 0x1D173..=0x1D17A
                | 0xE0000..=0xE0FFF
        )
}

/// What a person typed, without the invisible characters at either edge
/// (ADV-review-input-1). Text made only of invisible characters comes back
/// empty, so it is refused as missing, exactly like text left empty. An
/// invisible character *inside* visible text (a joiner in a word or an emoji)
/// is kept: only the edges go. Both doors reach this through [`record`].
pub fn visible_trim(text: &str) -> &str {
    text.trim_matches(is_invisible)
}

/// Whether this one character puts something a person can read on a screen
/// or in a terminal (ADV-review-input-4): its Unicode general category is a
/// letter (`L`), a number (`N`), punctuation (`P`) or a symbol (`S`) — the
/// Unicode 16 table in `finding_standing/legible.rs` — and it is not
/// [`is_invisible`], which takes out the Hangul fillers (letters that draw
/// nothing), the blank braille pattern and the object replacement character
/// (symbols that draw nothing).
/// Nothing else is: not white space, not a control or format character (the
/// interlinear annotation characters U+FFF9–U+FFFB among them, which Chrome
/// draws with no ink, and U+0600–U+0605, signs that belong to the number
/// after them), not a combining mark (`Mn`, `Mc`, `Me`), and not a
/// private-use or unassigned code point. The rule is by category, not by a
/// list of characters someone found. `review-model.js`'s `isLegible` is the
/// same rule, and `review-model.test.js` walks every code point to hold the
/// two equal.
pub fn is_legible(c: char) -> bool {
    !is_invisible(c) && legible::contains(c as u32)
}

/// What is kept of one typed part — the name, the authority, the reason —
/// and whether it counts as given at all (ADV-review-input-4): the
/// [`visible_trim`]med text when it holds at least one [`is_legible`]
/// character, and nothing otherwise. So a part made only of interlinear
/// annotation characters, of format characters or of combining marks is
/// refused as missing, exactly like one left empty, and where a move needs no
/// reason, nothing is saved for one nobody can read.
///
/// **A part made only of combining marks is not given.** A mark belongs to
/// the character before it; with none, a lone U+0301 is an accent over
/// nothing (a browser paints a small floating mark), not a name, a reason or
/// an authority, and it is refused as empty on both doors. A mark *on* a letter
/// is kept with its letter (`e` + U+0301 is saved as typed).
///
/// Both doors reach this through [`record`]; the terminal's `--actor` uses it
/// to decide whether a name was given at all.
pub fn visible_text(text: &str) -> &str {
    let trimmed = visible_trim(text);
    if trimmed.chars().any(is_legible) {
        trimmed
    } else {
        ""
    }
}

fn missing_parts(act: &StandingAct) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if visible_text(&act.actor).is_empty() {
        missing.push("actor");
    }
    if act.to.sets_aside() {
        if visible_text(&act.reason).is_empty() {
            missing.push("reason");
        }
        if visible_text(&act.authority).is_empty() {
            missing.push("authority");
        }
    }
    missing
}

/// Every typed part over [`MAX_PART_CHARS`], in the order `missing` names
/// them. Counted as characters, after [`visible_text`], because that is what
/// is stored.
fn too_long_parts(act: &StandingAct) -> Vec<&'static str> {
    [
        ("actor", &act.actor),
        ("reason", &act.reason),
        ("authority", &act.authority),
    ]
    .into_iter()
    .filter(|(_, text)| visible_text(text).chars().count() > MAX_PART_CHARS)
    .map(|(part, _)| part)
    .collect()
}

/// **The one write.** Both doors call this and nothing else.
///
/// Refusals are values, not errors: a person who left the reason empty is
/// told so, in the copy file's words, and nothing is written. An `Err` is
/// reserved for the store itself failing.
pub fn record(ledger: &Ledger, change_id: &str, act: &StandingAct) -> Result<Outcome> {
    let Some((review_id, ordinal)) = parse_finding_key(&act.finding) else {
        return Ok(Outcome::Refused(Refusal::because("no_such_finding")));
    };
    let mut world = EvidenceWorld::gather(ledger, change_id, &mut MaskRecord::default())?;
    let context = match finding_context(&world, change_id, &review_id, ordinal) {
        Ok(context) => context,
        Err(code) => return Ok(Outcome::Refused(Refusal::because(code))),
    };
    let (current, _) = current_standing(ledger, &review_id, ordinal)?;
    if let Some(seen) = act.seen {
        if seen != current {
            return Ok(Outcome::Refused(Refusal {
                code: "moved",
                missing: vec![],
                too_long: vec![],
                current: Some(current),
            }));
        }
    }
    // Rule 3 — a resolved mark is not a fix, and only a mark means "resolved".
    if act.via.is_resolved_mark() {
        if act.to != Standing::AwaitingVerification {
            return Ok(Outcome::Refused(Refusal::because("mark_goes_no_further")));
        }
        if !matches!(current, Standing::Open | Standing::UnderRepair) {
            return Ok(Outcome::Refused(Refusal {
                code: "mark_needs_an_open_finding",
                missing: vec![],
                too_long: vec![],
                current: Some(current),
            }));
        }
    } else if act.to == Standing::AwaitingVerification {
        return Ok(Outcome::Refused(Refusal::because("mark_goes_no_further")));
    }

    if act.to == current {
        return Ok(Outcome::Refused(Refusal {
            code: "unchanged",
            missing: vec![],
            too_long: vec![],
            current: Some(current),
        }));
    }

    // Rule 4 — every part that is missing, named together.
    let missing = missing_parts(act);
    if !missing.is_empty() {
        return Ok(Outcome::Refused(Refusal {
            code: "missing",
            missing,
            too_long: vec![],
            current: None,
        }));
    }
    // One limit for what a person types, whatever the move, named together.
    let too_long = too_long_parts(act);
    if !too_long.is_empty() {
        return Ok(Outcome::Refused(Refusal {
            code: "too_long",
            missing: vec![],
            too_long,
            current: None,
        }));
    }

    // Rule 2 — a fix needs evidence, and the evidence has to show it.
    let evidence = match (act.to, &act.evidence) {
        (Standing::FixedWithEvidence, None) => {
            return Ok(Outcome::Refused(Refusal::because("evidence_required")));
        }
        (Standing::FixedWithEvidence, Some(reference)) => {
            match acceptable_evidence(ledger, &mut world, &context, reference)? {
                Ok(view) => Some(view),
                Err(code) => return Ok(Outcome::Refused(Refusal::because(code))),
            }
        }
        (_, Some(_)) => return Ok(Outcome::Refused(Refusal::because("evidence_not_for_this"))),
        (_, None) => None,
    };

    let id = format!("fst_{}", ulid::Ulid::generate());
    let authority = visible_text(&act.authority);
    let tx = Transaction::new_unchecked(ledger.conn(), TransactionBehavior::Immediate)?;
    // Asked again inside the write transaction: two acts on one finding
    // cannot both start from the same standing. The trigger refuses it too.
    let (again, _) = current_standing(ledger, &review_id, ordinal)?;
    if again != current {
        return Ok(Outcome::Refused(Refusal {
            code: "moved",
            missing: vec![],
            too_long: vec![],
            current: Some(again),
        }));
    }
    tx.execute(
        "INSERT INTO change_review_finding_standings(id, change_id, review_id, ordinal,
             from_standing, to_standing, via, actor, authority, reason, evidence_kind,
             evidence_id, evidence_revision, recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![
            id,
            change_id,
            review_id,
            ordinal as i64,
            current.as_str(),
            act.to.as_str(),
            act.via.as_str(),
            visible_text(&act.actor),
            (!authority.is_empty()).then_some(authority),
            visible_text(&act.reason),
            evidence.as_ref().map(|e| e.kind.as_str()),
            evidence.as_ref().map(|e| e.id.as_str()),
            evidence.as_ref().map(|e| e.revision.as_str()),
            act.at,
        ],
    )?;
    tx.commit()?;
    Ok(Outcome::Recorded {
        id,
        finding: act.finding.clone(),
        from: current,
        to: act.to,
    })
}

// ---------------------------------------------------------------------------
// The read
// ---------------------------------------------------------------------------

/// One appended row, as served. Free text is masked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StandingRow {
    pub id: String,
    pub from: Standing,
    pub to: Standing,
    pub via: Via,
    pub actor: String,
    pub authority: Option<String>,
    pub reason: String,
    pub evidence_kind: Option<EvidenceKind>,
    pub evidence_id: Option<String>,
    pub evidence_revision: Option<String>,
    pub recorded_at: String,
}

/// One finding, where it stands, and what a person may do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingStandingView {
    /// [`finding_key`].
    pub key: String,
    pub review_id: String,
    pub ordinal: u32,
    pub review_round: u32,
    /// The version the finding was raised on.
    pub raised_on: String,
    pub kind: String,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub standing: Standing,
    /// False when no row exists: `open`, and nobody has decided anything.
    pub decided: bool,
    pub latest: Option<StandingRow>,
    /// Every row, oldest first. Nothing is edited, so this is the whole story.
    pub history: Vec<StandingRow>,
    /// The evidence behind a `fixed_with_evidence` standing, as it reads now.
    pub evidence: Option<EvidenceView>,
    /// Whether that evidence still applies to the version in front of you.
    /// `None` when the standing is not a fix.
    pub evidence_applies: Option<bool>,
    /// Why it no longer applies, as a refusal code, when it does not.
    pub evidence_lapsed: Option<&'static str>,
    /// True only for a fix whose evidence applies to the current version.
    pub counts_as_fixed: bool,
    pub group: Group,
    /// Records that would be accepted right now as evidence of a fix. Empty
    /// means nothing yet shows it was fixed, and the screen says so.
    pub fix_evidence: Vec<EvidenceView>,
    /// Whether a "resolved" mark can be recorded now.
    pub can_mark_resolved: bool,
    /// FIXED-WORDING (R-2026-09-28-9): for a `fixed_with_evidence` standing
    /// whose evidence is a review, every finding that review recorded — read
    /// from its stored findings, never inferred — so a screen can list them
    /// beside "Fixed" and a finding that came back renamed or reworded is in
    /// plain sight. `Some(empty)` means that re-check raised nothing; `None`
    /// means the standing is not a fix, the evidence is a check run (which
    /// raises no review findings), or the review could not be read.
    pub recheck_raised: Option<Vec<ReviewFinding>>,
}

/// How many findings are in each group. Typed counts, for a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct FindingsSummary {
    pub still_wrong: u32,
    pub awaiting_recheck: u32,
    pub fixed: u32,
    pub set_aside: u32,
    pub informational: u32,
}

/// Every finding of one Change and where it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingsView {
    pub schema_version: u32,
    pub change_id: String,
    pub current_revision: Option<String>,
    pub findings: Vec<FindingStandingView>,
    pub summary: FindingsSummary,
    pub masking: crate::redact::Treatment,
}

fn rows_for(
    ledger: &Ledger,
    change_id: &str,
) -> Result<BTreeMap<(String, u32), Vec<StandingRowRaw>>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, review_id, ordinal, from_standing, to_standing, via, actor, authority,
                reason, evidence_kind, evidence_id, evidence_revision, recorded_at
           FROM change_review_finding_standings WHERE change_id=?1 ORDER BY rowid",
    )?;
    let rows = statement.query_map(params![change_id], |row| {
        Ok(StandingRowRaw {
            id: row.get(0)?,
            review_id: row.get(1)?,
            ordinal: row.get::<_, i64>(2)?,
            from: row.get(3)?,
            to: row.get(4)?,
            via: row.get(5)?,
            actor: row.get(6)?,
            authority: row.get(7)?,
            reason: row.get(8)?,
            evidence_kind: row.get(9)?,
            evidence_id: row.get(10)?,
            evidence_revision: row.get(11)?,
            recorded_at: row.get(12)?,
        })
    })?;
    let mut out: BTreeMap<(String, u32), Vec<StandingRowRaw>> = BTreeMap::new();
    for row in rows {
        let row = row?;
        let ordinal = u32::try_from(row.ordinal)
            .map_err(|_| CoreError::Integrity("finding ordinal out of range".into()))?;
        out.entry((row.review_id.clone(), ordinal))
            .or_default()
            .push(row);
    }
    Ok(out)
}

struct StandingRowRaw {
    id: String,
    review_id: String,
    ordinal: i64,
    from: String,
    to: String,
    via: String,
    actor: String,
    authority: Option<String>,
    reason: String,
    evidence_kind: Option<String>,
    evidence_id: Option<String>,
    evidence_revision: Option<String>,
    recorded_at: String,
}

fn served_row(raw: &StandingRowRaw, masking: &mut MaskRecord) -> Result<StandingRow> {
    Ok(StandingRow {
        id: raw.id.clone(),
        from: Standing::parse(&raw.from)?,
        to: Standing::parse(&raw.to)?,
        via: Via::parse(&raw.via)?,
        actor: mask_text(&raw.actor, masking),
        authority: raw.authority.as_deref().map(|a| mask_text(a, masking)),
        reason: mask_text(&raw.reason, masking),
        evidence_kind: raw
            .evidence_kind
            .as_deref()
            .map(EvidenceKind::parse)
            .transpose()?,
        evidence_id: raw.evidence_id.clone(),
        evidence_revision: raw.evidence_revision.clone(),
        recorded_at: raw.recorded_at.clone(),
    })
}

/// Every finding of every intact, finished review of this Change, oldest
/// review first, each with where it stands. Reads; never writes.
pub fn read(ledger: &Ledger, change_id: &str) -> Result<FindingsView> {
    let mut masking = MaskRecord::default();
    let mut world = EvidenceWorld::gather(ledger, change_id, &mut masking)?;
    let rows = rows_for(ledger, change_id)?;
    let mut findings = Vec::new();
    let mut summary = FindingsSummary::default();

    let reviews: Vec<ReviewView> = world.reviews.clone();
    for review in &reviews {
        if review.state != ReviewState::Complete
            || review.integrity != crate::change_review::Integrity::Intact
        {
            continue;
        }
        for (index, finding) in review.findings.iter().enumerate() {
            let ordinal = u32::try_from(index)
                .map_err(|_| CoreError::Integrity("finding ordinal out of range".into()))?;
            let context = FindingContext {
                change_id: change_id.to_string(),
                review_round: review.review_round,
                raised_on: review.target_revision.clone(),
                examined_inputs_id: review.examined_inputs_id.clone(),
                diff_sha256: review.diff_sha256.clone(),
                path: finding.path.clone(),
                kind: finding.kind.clone(),
                message: finding.message.clone(),
            };
            let history_raw = rows.get(&(review.id.clone(), ordinal));
            let mut history: Vec<StandingRow> = Vec::new();
            for raw in history_raw.map(Vec::as_slice).unwrap_or(&[]) {
                history.push(served_row(raw, &mut masking)?);
            }
            let latest = history.last().cloned();
            let standing = latest.as_ref().map(|r| r.to).unwrap_or(Standing::Open);

            // A recorded fix is asked again: does its evidence still apply to
            // the version in front of you?
            let (evidence, evidence_applies, evidence_lapsed) = match (&latest, standing) {
                (Some(row), Standing::FixedWithEvidence) => {
                    match (row.evidence_kind, row.evidence_id.as_deref()) {
                        (Some(kind), Some(id)) => {
                            let reference = EvidenceRef {
                                kind,
                                id: id.to_string(),
                            };
                            match check_evidence(ledger, &mut world, &context, &reference)? {
                                Ok(view) => {
                                    (Some(masked_evidence(view, &mut masking)), Some(true), None)
                                }
                                Err(code) => (
                                    Some(EvidenceView {
                                        kind,
                                        id: id.to_string(),
                                        revision: row.evidence_revision.clone().unwrap_or_default(),
                                        review_round: None,
                                        name: String::new(),
                                    }),
                                    Some(false),
                                    Some(code),
                                ),
                            }
                        }
                        _ => (None, Some(false), Some("evidence_not_found")),
                    }
                }
                _ => (None, None, None),
            };
            // What the recorded re-check raised, whatever else it says: read
            // from the review the row names, when it is intact and finished.
            let recheck_raised = match (&latest, standing) {
                (Some(row), Standing::FixedWithEvidence)
                    if row.evidence_kind == Some(EvidenceKind::Review) =>
                {
                    row.evidence_id.as_deref().and_then(|id| {
                        reviews
                            .iter()
                            .find(|r| {
                                r.id == id
                                    && r.state == ReviewState::Complete
                                    && r.integrity == crate::change_review::Integrity::Intact
                            })
                            .map(|r| r.findings.clone())
                    })
                }
                _ => None,
            };
            let counts_as_fixed =
                standing == Standing::FixedWithEvidence && evidence_applies == Some(true);
            let group = match standing {
                Standing::Open | Standing::UnderRepair => Group::StillWrong,
                Standing::AwaitingVerification => Group::AwaitingRecheck,
                Standing::FixedWithEvidence if counts_as_fixed => Group::Fixed,
                Standing::FixedWithEvidence => Group::AwaitingRecheck,
                Standing::Disputed | Standing::Waived => Group::SetAside,
                Standing::Informational => Group::Informational,
            };
            match group {
                Group::StillWrong => summary.still_wrong += 1,
                Group::AwaitingRecheck => summary.awaiting_recheck += 1,
                Group::Fixed => summary.fixed += 1,
                Group::SetAside => summary.set_aside += 1,
                Group::Informational => summary.informational += 1,
            }

            // What would be accepted as evidence right now, from the same rule.
            let mut fix_evidence = Vec::new();
            if !counts_as_fixed {
                let later: Vec<String> = reviews
                    .iter()
                    .filter(|r| r.review_round > review.review_round)
                    .map(|r| r.id.clone())
                    .collect();
                for id in later {
                    let reference = EvidenceRef {
                        kind: EvidenceKind::Review,
                        id,
                    };
                    if let Ok(view) = acceptable_evidence(ledger, &mut world, &context, &reference)?
                    {
                        fix_evidence.push(masked_evidence(view, &mut masking));
                    }
                }
                if context.path.is_some() {
                    let runs: Vec<String> = world
                        .candidates(ledger, change_id)?
                        .iter()
                        .filter(|c| c.kind == crate::change_evidence::EvidenceKind::CheckRun)
                        .map(|c| c.id.clone())
                        .collect();
                    for id in runs {
                        let reference = EvidenceRef {
                            kind: EvidenceKind::CheckRun,
                            id,
                        };
                        if let Ok(view) =
                            acceptable_evidence(ledger, &mut world, &context, &reference)?
                        {
                            fix_evidence.push(masked_evidence(view, &mut masking));
                        }
                    }
                }
            }

            findings.push(FindingStandingView {
                key: finding_key(&review.id, ordinal),
                review_id: review.id.clone(),
                ordinal,
                review_round: review.review_round,
                raised_on: review.target_revision.clone(),
                kind: finding.kind.clone(),
                message: finding.message.clone(),
                path: finding.path.clone(),
                line: finding.line,
                standing,
                decided: latest.is_some(),
                latest,
                history,
                evidence,
                evidence_applies,
                evidence_lapsed,
                counts_as_fixed,
                group,
                fix_evidence,
                can_mark_resolved: matches!(standing, Standing::Open | Standing::UnderRepair),
                recheck_raised,
            });
        }
    }

    Ok(FindingsView {
        schema_version: FINDINGS_VIEW_SCHEMA_VERSION,
        change_id: change_id.to_string(),
        current_revision: world.current_revision.clone(),
        findings,
        summary,
        masking: masking.to_treatment(),
    })
}

// ---------------------------------------------------------------------------
// What a fix says (FIXED-WORDING, R-2026-09-28-9)
// ---------------------------------------------------------------------------

/// The founder's sentence, for a finding that names a file and a line. Word
/// for word `HQ_COPY.change.review.standing.fixed_at_line`; the three
/// templates below and the list's words are held equal to the copy golden by
/// `finding_standing_copy.rs`.
pub const FIXED_AT_LINE: &str =
    "Fixed: the re-check didn't raise this finding at {path}, line {line}";
/// The same, for a finding that names a file and no line.
pub const FIXED_AT_FILE: &str = "Fixed: the re-check didn't raise this finding at {path}";
/// The same, for a finding that names no file.
pub const FIXED_NO_PLACE: &str =
    "Fixed: the re-check didn't raise this finding (this finding names no file)";
/// The re-check raised nothing at all.
pub const RECHECK_NONE: &str = "The same re-check raised nothing else.";
/// The re-check raised one other thing.
pub const RECHECK_ONE: &str =
    "The same re-check raised 1 other thing. Look at it in case it is this one, reworded or moved:";
/// The re-check raised `{n}` other things.
pub const RECHECK_MANY: &str = "The same re-check raised {n} other things. Look at them in case one is this one, reworded or moved:";
/// Where a listed finding names no file.
pub const RECHECK_NO_PLACE: &str = "No file named";
/// The evidence is a check run, which raises no review findings.
pub const RECHECK_CHECK_RUN: &str = "That re-check was a check run, and a check run raises no review findings, so there is nothing else to list.";
/// The evidence is a review whose findings could not be read.
pub const RECHECK_UNREADABLE: &str =
    "What else that re-check raised cannot be listed: its review could not be read.";
/// A place in a file, as `HQ_COPY.change.review.finding_at` words it.
pub const FINDING_AT: &str = "{path}, line {line}";
/// A file, as `HQ_COPY.change.review.finding_in` words it.
pub const FINDING_IN: &str = "{path}";

fn place(path: Option<&str>, line: Option<u64>) -> Option<String> {
    let path = path.filter(|p| !p.is_empty())?;
    Some(match line {
        Some(line) => FINDING_AT
            .replace("{path}", path)
            .replace("{line}", &line.to_string()),
        None => FINDING_IN.replace("{path}", path),
    })
}

/// "Fixed: the re-check didn't raise this finding at <location>", with the
/// finding's recorded place, or its plain variant when it names no file.
pub fn fixed_sentence(path: Option<&str>, line: Option<u64>) -> String {
    match (path.filter(|p| !p.is_empty()), line) {
        (Some(path), Some(line)) => FIXED_AT_LINE
            .replace("{path}", path)
            .replace("{line}", &line.to_string()),
        (Some(path), None) => FIXED_AT_FILE.replace("{path}", path),
        (None, _) => FIXED_NO_PLACE.to_string(),
    }
}

/// What a person reads about a finding that stands `fixed_with_evidence`:
/// the founder's sentence, then every other thing the same re-check raised,
/// each with its place and its own words (indented as a list), or the plain
/// sentence saying there is none, or why none can be listed. Empty for any
/// other standing. The terminal door prints exactly these lines; the screen
/// composes the same ones from the copy file (`review-model.js`).
pub fn fixed_lines(view: &FindingStandingView) -> Vec<String> {
    if view.standing != Standing::FixedWithEvidence {
        return Vec::new();
    }
    let mut lines = vec![fixed_sentence(view.path.as_deref(), view.line)];
    let by_check = view
        .evidence
        .as_ref()
        .is_some_and(|e| e.kind == EvidenceKind::CheckRun);
    match (&view.recheck_raised, by_check) {
        (_, true) => lines.push(RECHECK_CHECK_RUN.to_string()),
        (None, false) => lines.push(RECHECK_UNREADABLE.to_string()),
        (Some(raised), false) => {
            match raised.len() {
                0 => lines.push(RECHECK_NONE.to_string()),
                1 => lines.push(RECHECK_ONE.to_string()),
                n => lines.push(RECHECK_MANY.replace("{n}", &n.to_string())),
            }
            for f in raised {
                let at = place(f.path.as_deref(), f.line)
                    .unwrap_or_else(|| RECHECK_NO_PLACE.to_string());
                let words = if f.message.is_empty() {
                    f.kind.clone()
                } else {
                    format!("{} — {}", f.kind, f.message)
                };
                lines.push(format!("  - {at}: {words}"));
            }
        }
    }
    lines
}

fn masked_evidence(view: EvidenceView, masking: &mut MaskRecord) -> EvidenceView {
    EvidenceView {
        name: mask_text(&view.name, masking),
        ..view
    }
}
