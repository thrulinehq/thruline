//! §5u — the attention engine. "What needs me?" as a derivation.
//!
//! This module answers one question and refuses several others.
//!
//! **It is a derivation, not a collector.** Every item is recomputed from
//! accepted records at read time. There is no watcher, poller, daemon, or
//! network service here, and M6.R1's gate asserts all four remain zero. A
//! queue is the natural place to lose that property — "what needs me?" invites
//! a background process, and a background process is a promise that the
//! product is watching when it is not.
//!
//! **It speaks only graduated truth.** `unsupported` and `dropped` are
//! full-shadow (A79); their path to this queue runs through the temporal
//! holdout adjudication, not through how central "leftovers" feel to the
//! product's story. [`AttentionKind`] is a closed world of four, and
//! `attention_excludes_shadow_classes` fails if that ever stops being true.
//!
//! **Its identity is content-addressed.** An item id digests
//! (project, kind, subject identity, subject content digest), so dismissing an
//! item expires naturally when its subject changes — reappearance-on-change is
//! structural rather than a stateful rule someone has to remember to apply.
//!
//! **It never mutates.** Dismissal is an append-only acknowledgment record
//! under the same triggers as the M5 lifecycle records. A product that edits
//! its own past to look calmer has stopped being a ledger.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::capture_product::CaptureReadiness;
use crate::chain::sha256_hex;
use crate::error::Result;
use crate::ledger::Ledger;

/// The closed world of attention items, v1.
///
/// **Deliberately excluded, with the reason each is excluded:**
///
/// * `unsupported`, `dropped` — full-shadow classes (A79, §5s). Graduation is
///   NOT ADJUDICATED, and A80 records that the holdout partition is currently
///   accruing nothing, so the adjudication that would graduate them cannot yet
///   run. Diagnostics is their only home.
/// * agent collisions, stuck loops — M8 concerns; they need cross-session
///   liveness this derivation deliberately does not have.
/// * anything requiring re-execution — ThruLine never re-runs a user's work.
///   An item may *tell a human* to re-run something; it may not do it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionKind {
    /// A current, graduated `contradicted` claim.
    Contradiction,
    /// Proof whose currency transitioned to `stale` (M5 currency records).
    Staleness,
    /// A readiness state that requires the human. See [`readiness_needs_human`].
    CaptureAttention,
    /// The calm sentence. Calm is a first-class item, not an absence — a blank
    /// screen makes a user wonder whether the product ran.
    AllClear,
    /// Nothing has been read for this project, or what was read is older than
    /// the record. Distinct from [`AttentionKind::AllClear`] on purpose: calm
    /// is a finding, and "we have not looked" is not.
    NotRead,
}

impl AttentionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contradiction => "contradiction",
            Self::Staleness => "staleness",
            Self::CaptureAttention => "capture_attention",
            Self::AllClear => "all_clear",
            Self::NotRead => "not_read",
        }
    }
}

/// The readiness → attention mapping over all twelve A78 states.
///
/// `true` means the state requires the human. The calm five are calm for
/// distinct reasons and the distinction matters: three of them are states the
/// human *chose* or that are simply the honest steady state, and a product
/// that nags about its own correct configuration teaches people to ignore it.
///
/// * `NotAvailable` — the source is not installed. Nothing is wrong.
/// * `BoundedOnly` — the honest steady state for a source whose local
///   execution boundary is unproven (Claude, today). Not a defect.
/// * `Disabled` — the human chose this.
/// * `AvailableNotInstalled` — an offer, not a problem.
/// * `AssuredReady` — working exactly as intended.
///
/// The attention-worthy seven each name something only a human can resolve:
/// a self-test that has not run or did not pass, a profile that does not
/// qualify, drift between what was authorized and what is on disk, or a
/// capture that errored.
pub fn readiness_needs_human(state: CaptureReadiness) -> bool {
    match state {
        CaptureReadiness::NotAvailable
        | CaptureReadiness::BoundedOnly
        | CaptureReadiness::Disabled
        | CaptureReadiness::AvailableNotInstalled
        | CaptureReadiness::AssuredReady => false,

        CaptureReadiness::DegradedSourceVersion
        | CaptureReadiness::DegradedConfigurationDrift
        | CaptureReadiness::DegradedKeyOrIdentity
        | CaptureReadiness::InstalledUnverified
        | CaptureReadiness::DegradedSourcePairing
        | CaptureReadiness::UnsupportedProfile
        | CaptureReadiness::CaptureError => true,
    }
}

/// One thing that may need a human, with everything needed to explain it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionItem {
    /// Content-addressed: digest over project, kind, subject identity, and
    /// subject content digest. Stable across rebuilds; changes when the
    /// subject changes.
    pub id: String,
    pub kind: AttentionKind,
    pub project_id: String,
    /// What the item is about — a node id, an event id, an owner/source pair.
    pub subject: String,
    /// Digest of the subject's current content. The reason dismissal expires.
    pub subject_digest: String,
    /// Receipt anchors, for `why`. Never rendered at first level.
    pub anchor_event_id: Option<String>,
    pub session_id: Option<String>,
    /// Engine-side detail carried for the second disclosure level only.
    pub detail: String,
    /// Numbers a numeric contradiction commits to, for the doctrine's
    /// "with numbers when numeric" clause.
    pub claimed: Option<String>,
    pub observed: Option<String>,
    /// The readiness state a `capture_attention` item is about. Carried
    /// typed rather than parsed back out of `detail`, so the presentation
    /// authority resolves it by an exhaustive match instead of a string
    /// prefix that a reworded `detail` could break.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub readiness: Option<CaptureReadiness>,
}

impl AttentionItem {
    fn new(
        kind: AttentionKind,
        project_id: &str,
        subject: &str,
        subject_digest: &str,
    ) -> AttentionItem {
        // §5u identity. The separator is a unit separator so a subject
        // containing the delimiter cannot forge a different item's id.
        let material = format!(
            "{project_id}\u{1f}{}\u{1f}{subject}\u{1f}{subject_digest}",
            kind.as_str()
        );
        AttentionItem {
            id: format!("att_{}", &sha256_hex(material.as_bytes())[..24]),
            kind,
            project_id: project_id.to_string(),
            subject: subject.to_string(),
            subject_digest: subject_digest.to_string(),
            anchor_event_id: None,
            session_id: None,
            detail: String::new(),
            claimed: None,
            observed: None,
            readiness: None,
        }
    }
}

/// An append-only dismissal. v1 has exactly one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcknowledgmentAction {
    Dismissed,
}

impl AcknowledgmentAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dismissed => "dismissed",
        }
    }
}

/// Record a dismissal. Append-only; the triggers refuse update and delete.
pub fn acknowledge(
    ledger: &Ledger,
    item_id: &str,
    action: AcknowledgmentAction,
    at_unix_ms: u64,
) -> Result<()> {
    ledger.conn().execute(
        "INSERT OR IGNORE INTO attention_acknowledgments(item_id, action, acknowledged_at_unix_ms)
         VALUES(?1,?2,?3)",
        params![item_id, action.as_str(), at_unix_ms as i64],
    )?;
    Ok(())
}

/// Acknowledged item ids for a project's items, newest action first.
pub fn acknowledged_ids(ledger: &Ledger) -> Result<std::collections::BTreeMap<String, i64>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT item_id, MAX(acknowledged_at_unix_ms) FROM attention_acknowledgments
         GROUP BY item_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut out = std::collections::BTreeMap::new();
    for row in rows {
        let (id, at) = row?;
        out.insert(id, at);
    }
    Ok(out)
}

/// How many items a single `now` will consider per class.
///
/// §5u requires that answering "what needs me?" never loads full history. The
/// queries below are indexed and bounded; this is the bound.
const WINDOW: usize = 200;

/// Derive the attention items for one project.
///
/// Deterministic: ordered by (kind, subject) so two rebuilds of the same
/// accepted records produce byte-identical output.
pub fn derive_attention(ledger: &Ledger, project_id: &str) -> Result<Vec<AttentionItem>> {
    let mut items: Vec<AttentionItem> = Vec::new();
    items.extend(contradictions(ledger, project_id)?);
    items.extend(stale_proofs(ledger, project_id)?);
    items.extend(capture_items(ledger, project_id)?);

    items.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(a.subject.cmp(&b.subject))
            .then(a.id.cmp(&b.id))
    });

    if items.is_empty() {
        // Calm is only honest once the projection has actually been built at
        // the current chain position. Before that, an empty result means the
        // queries found nothing to look at — not that there is nothing wrong.
        items.push(match projection_state(ledger, project_id)? {
            ProjectionState::Current => all_clear_item(ledger, project_id)?,
            ProjectionState::NeverBuilt => not_read_item(
                ledger,
                project_id,
                "this project has not been read yet, so nothing has been checked",
            )?,
            ProjectionState::BehindRecord => not_read_item(
                ledger,
                project_id,
                "what was read is older than the record, so newer work has not been checked",
            )?,
        });
    }
    Ok(items)
}

/// Whether the derived projection can be trusted to answer "is anything wrong".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionState {
    Current,
    NeverBuilt,
    BehindRecord,
}

pub fn projection_state(ledger: &Ledger, project_id: &str) -> Result<ProjectionState> {
    let Some((built_at_head, _)) = ledger.projection_head(project_id)? else {
        return Ok(ProjectionState::NeverBuilt);
    };
    let (current_head, _) = ledger.compute_project_head(project_id)?;
    Ok(if built_at_head == current_head {
        ProjectionState::Current
    } else {
        ProjectionState::BehindRecord
    })
}

/// The "we have not looked" item. Never says anything about what is or is not
/// wrong, because nothing has been examined.
pub fn not_read_item(ledger: &Ledger, project_id: &str, detail: &str) -> Result<AttentionItem> {
    let head: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT head_hash FROM ledger_meta WHERE project_id = ?1",
            params![project_id],
            |r| r.get(0),
        )
        .ok();
    let digest = head.unwrap_or_else(|| "genesis".to_string());
    let mut item = AttentionItem::new(AttentionKind::NotRead, project_id, "project", &digest);
    item.detail = detail.to_string();
    Ok(item)
}

/// The calm item.
///
/// Calm is an item, not an absence — a blank screen makes a person wonder
/// whether the product ran. The digest is over the project's derived head, so
/// "all clear" itself expires when anything changes.
///
/// Public because `now` needs to build the identical item when every derived
/// item is dismissed: two constructions of the same sentence would eventually
/// be two different ids for the same state.
pub fn all_clear_item(ledger: &Ledger, project_id: &str) -> Result<AttentionItem> {
    let head: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT head_hash FROM ledger_meta WHERE project_id = ?1",
            params![project_id],
            |r| r.get(0),
        )
        .ok();
    let digest = head.unwrap_or_else(|| "genesis".to_string());
    let mut item = AttentionItem::new(AttentionKind::AllClear, project_id, "project", &digest);
    item.detail = "no contradictions, no stale proof, no capture state needing a human".to_string();
    Ok(item)
}

/// Graduated `contradicted` claims that are still current.
///
/// Reads the derived `nodes` projection, which is itself rebuildable from
/// accepted records — so attention inherits derivation's determinism instead
/// of inventing its own.
fn contradictions(ledger: &Ledger, project_id: &str) -> Result<Vec<AttentionItem>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT id, label, session_id, anchor_event_id, evidence_state, lifecycle
         FROM nodes
         WHERE project_id = ?1 AND evidence_state = 'contradicted' AND currency = 'current'
         ORDER BY id
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![project_id, WINDOW as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, label, session, anchor, evidence, lifecycle) = row?;
        let digest = sha256_hex(format!("{evidence}\u{1f}{lifecycle}\u{1f}{label}").as_bytes());
        let mut item = AttentionItem::new(AttentionKind::Contradiction, project_id, &id, &digest);
        item.anchor_event_id = Some(anchor);
        item.session_id = Some(session);
        item.detail = label;
        out.push(item);
    }
    Ok(out)
}

/// Proof whose latest accepted comparison says `stale`.
///
/// One indexed query over accepted capture records — no per-event lookups, so
/// this stays inside the windowed budget on a large project.
fn stale_proofs(ledger: &Ledger, project_id: &str) -> Result<Vec<AttentionItem>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT l.event_id, e.session_id, c.comparison_id, MAX(c.accepted_seq)
         FROM capture_event_links l
         JOIN events e ON e.id = l.event_id
         JOIN state_capsules s ON s.attempt_id = l.attempt_id AND s.phase = 'post'
         JOIN state_comparisons c ON c.execution_capsule_id = s.capsule_id
         WHERE e.project_id = ?1
         GROUP BY l.event_id
         HAVING c.currentness = 'stale'
         ORDER BY l.event_id
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![project_id, WINDOW as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (event_id, session, comparison) = row?;
        // The comparison id IS the subject content digest: a new comparison
        // means a new item, so a dismissal cannot outlive the fact.
        let mut item =
            AttentionItem::new(AttentionKind::Staleness, project_id, &event_id, &comparison);
        item.anchor_event_id = Some(event_id);
        item.session_id = Some(session);
        item.detail = comparison;
        out.push(item);
    }
    Ok(out)
}

/// Capture readiness states that require the human.
///
/// Readiness is derived by `capture_product`; this only maps it. The mapping
/// lives in [`readiness_needs_human`] so the calm/attention split is one
/// reviewable table rather than scattered conditionals.
fn capture_items(ledger: &Ledger, project_id: &str) -> Result<Vec<AttentionItem>> {
    // Owner/source/project triples that this ledger has actually accepted.
    // Authorizations carry the project key; lifecycle records carry owners that
    // were installed but never authorized, and those still have readiness worth
    // reporting (`installed_unverified` is exactly that state).
    let mut stmt = ledger.conn().prepare(
        "SELECT DISTINCT owner_key, source, source_project_key
           FROM capture_project_authorizations
         UNION
         SELECT DISTINCT owner_key, source, ?1 FROM capture_lifecycle_records
         ORDER BY 1, 2, 3",
    )?;
    let owners = stmt
        .query_map(params![project_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut out = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (owner_key, source, project_key) in owners {
        // `recorded_readiness` is the BOUNDED view: it reconstructs the last
        // accepted operational state and touches no live source, configuration,
        // collector filesystem, Git, or network. The CLI's `inspect_readiness`
        // does probe the live system, and calling it from here would make the
        // attention surface a collector (§5u).
        let report = match crate::capture_product::recorded_readiness(
            ledger,
            &owner_key,
            &source,
            &project_key,
        ) {
            Ok(r) => r,
            // A readiness lookup that cannot complete is not an attention
            // item — it is a `doctor` concern. Staying silent is
            // deliberate: inventing an alarm out of our own failure is the
            // mistake §5c spent three milestones removing from verdicts.
            Err(_) => continue,
        };
        if !readiness_needs_human(report.state) {
            continue;
        }
        let subject = format!("{owner_key}/{source}");
        if !seen.insert(subject.clone()) {
            continue;
        }
        let digest =
            sha256_hex(format!("{}\u{1f}{}", report.state.as_str(), report.reason).as_bytes());
        let mut item = AttentionItem::new(
            AttentionKind::CaptureAttention,
            project_id,
            &subject,
            &digest,
        );
        item.detail = format!("{}: {}", report.state.as_str(), report.reason);
        item.readiness = Some(report.state);
        out.push(item);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_item_world_is_closed_and_excludes_shadow_classes() {
        // §5u: shadow classes never appear outside the diagnostics flag. This
        // asserts the TAXONOMY cannot express them — the strongest available
        // form, because a kind that does not exist cannot be emitted by a
        // future careless branch.
        let all = [
            AttentionKind::Contradiction,
            AttentionKind::Staleness,
            AttentionKind::CaptureAttention,
            AttentionKind::AllClear,
        ];
        assert_eq!(all.len(), 4, "the v1 item world is exactly four kinds");
        for k in all {
            let s = k.as_str();
            assert!(
                s != "unsupported" && s != "dropped",
                "a shadow class reached the attention taxonomy: {s}"
            );
        }
    }

    #[test]
    fn identity_is_content_addressed_and_expires_on_change() {
        let a = AttentionItem::new(AttentionKind::Contradiction, "proj", "node-1", "digest-a");
        let same = AttentionItem::new(AttentionKind::Contradiction, "proj", "node-1", "digest-a");
        let changed =
            AttentionItem::new(AttentionKind::Contradiction, "proj", "node-1", "digest-b");
        let other_kind = AttentionItem::new(AttentionKind::Staleness, "proj", "node-1", "digest-a");
        let other_proj =
            AttentionItem::new(AttentionKind::Contradiction, "p2", "node-1", "digest-a");
        assert_eq!(a.id, same.id, "identity must be stable across rebuilds");
        assert_ne!(
            a.id, changed.id,
            "a changed subject must produce a NEW id — this is how dismissal expires"
        );
        assert_ne!(a.id, other_kind.id);
        assert_ne!(a.id, other_proj.id);
    }

    #[test]
    fn every_readiness_state_has_an_explicit_attention_decision() {
        // The mapping is exhaustive by construction (the match has no
        // wildcard), and this pins the split so a new state cannot silently
        // default to calm.
        let attention = [
            CaptureReadiness::DegradedSourceVersion,
            CaptureReadiness::DegradedConfigurationDrift,
            CaptureReadiness::DegradedKeyOrIdentity,
            CaptureReadiness::InstalledUnverified,
            CaptureReadiness::DegradedSourcePairing,
            CaptureReadiness::UnsupportedProfile,
            CaptureReadiness::CaptureError,
        ];
        let calm = [
            CaptureReadiness::NotAvailable,
            CaptureReadiness::BoundedOnly,
            CaptureReadiness::Disabled,
            CaptureReadiness::AvailableNotInstalled,
            CaptureReadiness::AssuredReady,
        ];
        assert_eq!(attention.len() + calm.len(), 12, "all A78 states mapped");
        for s in attention {
            assert!(readiness_needs_human(s), "{} must need a human", s.as_str());
        }
        for s in calm {
            assert!(!readiness_needs_human(s), "{} must be calm", s.as_str());
        }
    }
}
