//! A187 — the adjudication instrument. **It builds the surface; it never
//! renders the judgment.**
//!
//! Every other derivation in this crate ends with the machine having decided
//! something. This module ends with the machine having made it possible for a
//! *human* to decide, and having recorded that a human did. There is no default
//! verdict, no seeded answer, and no inference of a likely one anywhere below —
//! an adjudication with no human behind it is worse than no adjudication,
//! because it launders a guess into the ledger wearing a human's authority.
//! Rule 5's project override is directly on point: judgment lives only in the
//! annotation layer, and this is where a human's judgment enters.
//!
//! **What it serves, and what it cannot.** A187's probe measured that a
//! "pending batch" is not a concept this codebase had — no table, no verb, no
//! state. It also measured that of RB1's 23 specimens only 15 are
//! re-identifiable by a live query: the other 8 are claude sessions selected by
//! having a non-meta tail, and 150 sessions derive `abandoned` today carrying an
//! **identical** basis string, so nothing the surface emits can separate them.
//! They are NAMED by [`unreachable_batches`] rather than silently absent — the
//! same discipline C8 applies to the classes it cannot build.
//!
//! **Why specimen identity is content-addressed.** A verdict recorded against a
//! specimen the system cannot find again is a verdict about nothing, and a
//! verdict that silently outlives a change in its subject is worse. The id
//! digests the class, the subject, the derived label and the evidence, so a
//! changed specimen is a *new* specimen and its old verdict does not transfer.
//! This is `attention.rs`'s identity rule, reused rather than reinvented.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::chain::sha256_hex;
use crate::error::Result;
use crate::ledger::Ledger;

/// A human's verdict on a derived hypothesis.
///
/// Three outcomes, and the third is **not** a lesser version of the other two.
/// "I cannot tell from this" is a real and valuable answer about the evidence,
/// and it is recorded as itself. The C4.3 precedent binds: **abstention is
/// never a pass** — it does not count toward agreement, and
/// [`counts_toward_graduation`] is where that is enforced rather than
/// remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The derived label is right.
    Agree,
    /// The derived label is wrong.
    Disagree,
    /// The evidence shown does not settle it. An answer, not a skip.
    InsufficientEvidence,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agree => "agree",
            Self::Disagree => "disagree",
            Self::InsufficientEvidence => "insufficient_evidence",
        }
    }

    /// Parse a typed answer. Returns `None` for anything else — including the
    /// empty string, because a bare Return must never become a verdict.
    pub fn parse(input: &str) -> Option<Verdict> {
        match input.trim().to_ascii_lowercase().as_str() {
            "a" | "agree" => Some(Self::Agree),
            "d" | "disagree" => Some(Self::Disagree),
            "i" | "insufficient" | "insufficient_evidence" => Some(Self::InsufficientEvidence),
            _ => None,
        }
    }

    /// Whether this verdict is an independent adjudication for graduation
    /// arithmetic.
    ///
    /// Abstention is not. A bar of "300 independent held-out adjudications"
    /// counted with abstentions in it would be a bar met by looking at 300
    /// specimens and declining to judge them.
    pub fn counts_toward_graduation(self) -> bool {
        !matches!(self, Self::InsufficientEvidence)
    }
}

/// One thing awaiting a human, with everything needed to judge it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Specimen {
    /// Content-addressed over class, subject, derived label and evidence.
    pub id: String,
    /// The class whose graduation this adjudication feeds. Carried per row
    /// because the graduation bar is per class.
    pub class: String,
    /// What is being judged — a session id, today.
    pub subject: String,
    /// What the machine derived. The thing the human is agreeing with or not.
    pub derived_label: String,
    /// The rule that produced it, in words, including its assumptions.
    pub basis: String,
    /// The rows it stands on. A specimen with no evidence is not shown: the
    /// receipt path back to the record is the point of this product, so the
    /// specimen carries its evidence rather than a summary of it.
    pub evidence: Vec<String>,
}

impl Specimen {
    pub fn new(
        class: &str,
        subject: &str,
        derived_label: &str,
        basis: &str,
        evidence: Vec<String>,
    ) -> Specimen {
        // Unit separators, so a field containing the delimiter cannot forge
        // another specimen's identity (the §5u rule).
        let material = format!(
            "{class}\u{1f}{subject}\u{1f}{derived_label}\u{1f}{}",
            sha256_hex(evidence.join("\u{1f}").as_bytes())
        );
        Specimen {
            id: format!("adj_{}", &sha256_hex(material.as_bytes())[..24]),
            class: class.to_string(),
            subject: subject.to_string(),
            derived_label: derived_label.to_string(),
            basis: basis.to_string(),
            evidence,
        }
    }
}

/// Batches that are pending but cannot be served, each with the reason.
///
/// Named rather than omitted. A surface that lists what it can reach and stays
/// silent about what it cannot is the false calm this product exists to refuse
/// — the same argument C8 makes for its unbuildable classes.
pub fn unreachable_batches() -> Vec<(&'static str, &'static str)> {
    vec![(
        "rb1 claude non-meta tails (8 specimens)",
        "not re-identifiable: 150 sessions derive `abandoned` today and all carry an identical \
         basis string, and the tail kind that selected these 8 is not a field the surface emits. \
         They exist only as a frozen list in docs/recon/rb1-shadow-status.json. Addressing them \
         is registered as Q-A187.1 and is a contract decision, not a CLI one.",
    )]
}

/// Every specimen that is pending and re-identifiable, in deterministic order.
///
/// Ordering is (class, subject) so two runs present the same queue in the same
/// order — an operator part-way through a batch must not have it reshuffle.
pub fn pending(ledger: &Ledger, now_utc: &str) -> Result<Vec<Specimen>> {
    let mut out: Vec<Specimen> = Vec::new();

    // C8's batch. Re-identifiable exactly: the surface prints session ids.
    for c in crate::guardian::derive(ledger, now_utc)? {
        out.push(Specimen::new(
            c.class.as_str(),
            &c.session_id,
            c.reason_code,
            &c.basis,
            c.evidence.clone(),
        ));
    }

    // RB1's batch, restricted to what a live query can find again: the codex
    // vendor-declared aborts. The claude half is reported by
    // `unreachable_batches` instead of guessed at.
    for row in crate::session_status::derive_all(ledger, now_utc)? {
        if row.source != "codex" || row.status.as_str() != "interrupted" {
            continue;
        }
        let mut evidence = vec![format!(
            "session {} in project {} derives {}",
            row.session_id,
            row.project_id,
            row.status.as_str()
        )];
        if let Some(ts) = row.last_ts.as_deref() {
            evidence.push(format!("last record at {ts}"));
        }
        out.push(Specimen::new(
            "session_status",
            &row.session_id,
            row.status.as_str(),
            row.basis,
            evidence,
        ));
    }

    out.sort_by(|a, b| a.class.cmp(&b.class).then(a.subject.cmp(&b.subject)));
    Ok(out)
}

/// A recorded adjudication, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorded {
    pub specimen_id: String,
    pub class: String,
    pub verdict: Verdict,
    /// Who. Never inferred silently — the caller resolves it and the surface
    /// prints it before anything is written.
    pub operator: String,
    /// By what route. `cli:adjudicate` today; the field exists so a second
    /// route can never be mistaken for this one.
    pub route: String,
    pub adjudicated_at_unix_ms: i64,
}

/// Append one adjudication. Append-only; the triggers refuse update and delete.
///
/// Idempotent by refusal, not by overwrite: a specimen that already carries a
/// verdict is left exactly as it was and `false` is returned. Silently
/// overwriting a human's recorded judgment is the one thing this store may
/// never do.
pub fn record(
    ledger: &Ledger,
    specimen: &Specimen,
    verdict: Verdict,
    operator: &str,
    route: &str,
    at_unix_ms: i64,
) -> Result<bool> {
    if verdict_for(ledger, &specimen.id)?.is_some() {
        return Ok(false);
    }
    ledger.conn().execute(
        "INSERT INTO adjudications(specimen_id, class, subject, verdict, operator, route, adjudicated_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            specimen.id,
            specimen.class,
            specimen.subject,
            verdict.as_str(),
            operator,
            route,
            at_unix_ms
        ],
    )?;
    Ok(true)
}

/// The verdict already recorded for a specimen, if any.
pub fn verdict_for(ledger: &Ledger, specimen_id: &str) -> Result<Option<Recorded>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT specimen_id, class, verdict, operator, route, adjudicated_at_unix_ms
           FROM adjudications WHERE specimen_id = ?1
          ORDER BY adjudicated_at_unix_ms ASC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![specimen_id])?;
    let Some(r) = rows.next()? else {
        return Ok(None);
    };
    let verdict = match r.get::<_, String>(2)?.as_str() {
        "agree" => Verdict::Agree,
        "disagree" => Verdict::Disagree,
        _ => Verdict::InsufficientEvidence,
    };
    Ok(Some(Recorded {
        specimen_id: r.get(0)?,
        class: r.get(1)?,
        verdict,
        operator: r.get(3)?,
        route: r.get(4)?,
        adjudicated_at_unix_ms: r.get(5)?,
    }))
}

/// Adjudications per class that count toward graduation.
///
/// Abstentions are excluded here, once, rather than at each call site.
pub fn counted_by_class(ledger: &Ledger) -> Result<std::collections::BTreeMap<String, u64>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT class, COUNT(*) FROM adjudications
          WHERE verdict <> 'insufficient_evidence' GROUP BY class",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
    })?;
    let mut out = std::collections::BTreeMap::new();
    for row in rows {
        let (class, n) = row?;
        out.insert(class, n);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abstention_is_an_answer_and_never_counts_as_agreement() {
        // C4.3's precedent, applied to the graduation arithmetic: a bar of 300
        // independent adjudications counted with abstentions in it would be a
        // bar met by looking at 300 specimens and declining to judge them.
        assert!(Verdict::Agree.counts_toward_graduation());
        assert!(Verdict::Disagree.counts_toward_graduation());
        assert!(!Verdict::InsufficientEvidence.counts_toward_graduation());
        assert_ne!(Verdict::InsufficientEvidence, Verdict::Agree);
    }

    #[test]
    fn a_bare_return_is_not_a_verdict() {
        // The whole item in one assertion: no input a human did not type may
        // become a judgment.
        assert_eq!(Verdict::parse(""), None);
        assert_eq!(Verdict::parse("   "), None);
        assert_eq!(Verdict::parse("y"), None);
        assert_eq!(Verdict::parse("yes"), None);
        assert_eq!(Verdict::parse("a"), Some(Verdict::Agree));
        assert_eq!(Verdict::parse("Disagree"), Some(Verdict::Disagree));
        assert_eq!(
            Verdict::parse("insufficient"),
            Some(Verdict::InsufficientEvidence)
        );
    }

    #[test]
    fn specimen_identity_expires_when_the_specimen_changes() {
        let a = Specimen::new("c", "s1", "crash", "b", vec!["e1".into()]);
        let same = Specimen::new("c", "s1", "crash", "b", vec!["e1".into()]);
        let new_evidence = Specimen::new("c", "s1", "crash", "b", vec!["e2".into()]);
        let new_label = Specimen::new("c", "s1", "other", "b", vec!["e1".into()]);
        assert_eq!(a.id, same.id, "identity must be stable across runs");
        assert_ne!(
            a.id, new_evidence.id,
            "changed evidence must produce a new specimen — a verdict must not \
             silently outlive the thing it judged"
        );
        assert_ne!(a.id, new_label.id);
    }

    #[test]
    fn the_unreachable_batch_is_named_with_its_reason() {
        let named = unreachable_batches();
        assert_eq!(named.len(), 1);
        assert!(named[0].0.contains("8"), "the count must be stated");
        assert!(
            named[0].1.contains("Q-A187.1"),
            "an unreachable batch cites the open question rather than shrugging"
        );
    }
}
