//! The two questions — *what needs me?* and *why should I believe it?*
//!
//! `attention.rs` derives the items; this module is what a person meets. Every
//! string here comes from [`crate::voice`], so the CLI prints and the R2 UI
//! renders the same sentences from the same authority rather than two
//! translations of one engine.
//!
//! **Read-only, recomputed on read.** Both entry points are pure queries over
//! accepted records. Nothing here schedules, watches, polls, or reaches the
//! network, and nothing here probes the live system: capture readiness arrives
//! through `recorded_readiness`, the bounded reconstruction, never through the
//! CLI's live `inspect_readiness`. §5u's `0/0/0/0` is a property of this path,
//! not a promise about it.
//!
//! **Three levels, named so no surface invents a fourth.** The sentence and its
//! one action; then what happened; then the receipt. `why` prints all three;
//! `now` prints the first. The R2 UI renders exactly this shape.

use rusqlite::params;
use serde::Serialize;

use crate::attention::{
    acknowledged_ids, all_clear_item, derive_attention, AttentionItem, AttentionKind,
};
use crate::error::Result;
use crate::ledger::Ledger;
use crate::voice;

/// One item as a person meets it: one sentence, one suggested action.
///
/// The suggested action is an instruction to a human. Nothing on this struct
/// can be executed, and `attention_suggestions_cannot_execute` asserts the
/// whole path has no way to try.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NowItem {
    pub id: String,
    pub kind: AttentionKind,
    /// Level one.
    pub sentence: String,
    pub action: String,
    /// Present only when the item is being shown despite a dismissal (`--all`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismissed_at_unix_ms: Option<i64>,
    /// The engine's own name for what this is about. Level two, carried so
    /// `why <id>` is reachable without a second derivation.
    pub subject: String,
}

/// One project's queue: the headline, then the items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectNow {
    pub project_id: String,
    /// The one sentence above the items.
    pub headline: String,
    pub items: Vec<NowItem>,
    /// How many items the default view is hiding because they were dismissed.
    pub dismissed_hidden: u64,
    /// The line that keeps a calm headline honest when a dismissal is hiding
    /// something. `None` at zero — a zero-count line is the paper cut §4 exists
    /// to stop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_note: Option<String>,
}

fn as_now_item(item: &AttentionItem, dismissed_at: Option<i64>) -> NowItem {
    NowItem {
        id: item.id.clone(),
        kind: item.kind,
        sentence: voice::attention_sentence(
            item.kind,
            item.claimed.as_deref(),
            item.observed.as_deref(),
            item.readiness,
        ),
        action: voice::attention_action(item.kind, item.readiness),
        dismissed_at_unix_ms: dismissed_at,
        subject: item.subject.clone(),
    }
}

/// What a person calls this project.
///
/// The same source `export` uses for the UI's project name (§9), so the
/// headline and the app agree. Falls back to the ledger key rather than
/// inventing a name.
fn project_name(ledger: &Ledger, project_id: &str) -> String {
    ledger
        .conn()
        .query_row(
            "SELECT display_name FROM projects WHERE id = ?1",
            params![project_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
        .unwrap_or_else(|| project_id.to_string())
}

/// What one project's queue actually contains, dismissals applied.
struct Visible {
    /// Each item with the dismissal that covers it, if any.
    items: Vec<(AttentionItem, Option<i64>)>,
    /// Items a default view is hiding because they were dismissed.
    hidden: u64,
    /// Items that genuinely need a person — the headline's count. A dismissed
    /// item is not one, whether or not it is being displayed.
    needing_attention: u64,
}

/// The one list both questions read.
///
/// `now` renders it; `why` resolves ids against it. Deriving the visible set
/// twice would eventually let the two surfaces disagree about what exists —
/// which is exactly how a product ends up listing an item whose "why" says no
/// such item exists.
///
/// Dismissed items are hidden by default and counted, never disappeared. The
/// calm item is never hidden by a dismissal: dismissing your way to a blank
/// screen is not a state this product has.
fn visible(
    ledger: &Ledger,
    project_id: &str,
    acks: &std::collections::BTreeMap<String, i64>,
    include_dismissed: bool,
) -> Result<Visible> {
    let mut out = Visible {
        items: Vec::new(),
        hidden: 0,
        needing_attention: 0,
    };
    for item in derive_attention(ledger, project_id)? {
        let dismissed = acks.get(&item.id).copied();
        // NotRead is deliberately NOT calm: we have not looked.
        let calm = item.kind == AttentionKind::AllClear;
        match dismissed {
            // The calm item ignores its own dismissal, deliberately.
            Some(_) if !calm && !include_dismissed => out.hidden += 1,
            Some(at) if !calm => out.items.push((item, Some(at))),
            _ => {
                if !calm {
                    out.needing_attention += 1;
                }
                out.items.push((item, None));
            }
        }
    }
    // Everything derived was dismissed: the queue is calm, and it says so.
    if out.items.is_empty() {
        out.items.push((all_clear_item(ledger, project_id)?, None));
    }
    Ok(out)
}

/// Answer "what needs me?" for one project or for every project.
/// Whether the recorder was actually watching the machine this record came from.
///
/// Core never observes the machine; it is *told*. `doctor` is the caller that
/// knows — it reads the installed hook set and the ledger's own runner evidence
/// — and it hands the answer here so that the sentence core composes can never
/// claim more than what was read. Passing [`Coverage::Complete`] is an
/// assertion, not a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Every coverage check the caller ran came back ok.
    Complete,
    /// At least one did not. The calm headline is refused.
    Incomplete,
}

pub fn now(
    ledger: &Ledger,
    project: Option<&str>,
    include_dismissed: bool,
    coverage: Coverage,
) -> Result<Vec<ProjectNow>> {
    let projects = match project {
        Some(p) => vec![p.to_string()],
        None => ledger.project_ids()?,
    };
    let acks = acknowledged_ids(ledger)?;

    let mut out = Vec::with_capacity(projects.len());
    for project_id in projects {
        let v = visible(ledger, &project_id, &acks, include_dismissed)?;
        out.push(ProjectNow {
            headline: if v.items.iter().all(|i| i.0.kind == AttentionKind::NotRead)
                && !v.items.is_empty()
            {
                voice::headline_not_read(&project_name(ledger, &project_id))
            } else if coverage == Coverage::Incomplete {
                // The record is not known to be complete, so "everything
                // checks out" is not available to say. Refused here, at the
                // one place the sentence is composed, rather than corrected
                // in a view that a later view could forget to correct.
                voice::headline_uncovered(&project_name(ledger, &project_id), v.needing_attention)
            } else {
                voice::headline(&project_name(ledger, &project_id), v.needing_attention)
            },
            project_id,
            items: v
                .items
                .iter()
                .map(|(item, at)| {
                    let mut n = as_now_item(item, *at);
                    // The item beneath a gated headline must not restate the
                    // claim the headline just refused.
                    if coverage == Coverage::Incomplete && n.kind == AttentionKind::AllClear {
                        n.sentence = voice::all_clear_uncovered_sentence().to_string();
                        n.action = voice::all_clear_uncovered_action().to_string();
                    }
                    n
                })
                .collect(),
            dismissed_hidden: v.hidden,
            hidden_note: voice::hidden_note(v.hidden),
        });
    }
    Ok(out)
}

/// A pointer back to a raw line of a transcript. Level three, unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Receipt {
    pub event_id: String,
    pub session_id: String,
    pub raw_file: String,
    pub raw_line: i64,
    pub raw_sha256: String,
    pub chain_hash: String,
}

/// One item, disclosed in three levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WhyView {
    pub id: String,
    pub project_id: Option<String>,
    /// `None` for an id that no longer derives — its kind is not recoverable
    /// from an acknowledgment record, and guessing one would be worse than
    /// saying so.
    pub kind: Option<AttentionKind>,
    /// Level one.
    pub sentence: String,
    pub action: String,
    /// Level two — our prose, from the voice authority.
    pub what_happened: String,
    /// Level two — the project's own words. A quotation is the project
    /// speaking, not us, so the lexicon gate scans `what_happened` and leaves
    /// this alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quoted: Option<String>,
    /// Level three.
    pub receipts: Vec<Receipt>,
    /// Level three — engine record ids that are not events (comparison ids,
    /// readiness reasons).
    pub records: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismissed_at_unix_ms: Option<i64>,
    /// True when the id resolves only from acknowledgment history.
    pub no_longer_derives: bool,
}

/// What a prefix resolved to. Ambiguity is a refusal, never a guess.
#[derive(Debug)]
pub enum Resolution {
    One(Box<WhyView>),
    /// Every id the prefix matched, so the human can retype a longer one.
    Ambiguous(Vec<String>),
    NotFound,
}

/// Look up one attention item by id or unambiguous prefix.
///
/// Answers for dismissed items and for ids that no longer derive: a dismissal
/// record is permanent, so the question stays answerable. History is never
/// rewritten to make an answer unavailable.
pub fn why(ledger: &Ledger, id_or_prefix: &str) -> Result<Resolution> {
    let needle = id_or_prefix.trim();
    if needle.is_empty() {
        return Ok(Resolution::NotFound);
    }
    let acks = acknowledged_ids(ledger)?;

    // Resolve against everything the product can display under EITHER flag:
    // `--all` shows dismissed items, the default substitutes the calm item for
    // them, and `why` has to answer for every id a person can see. Taking only
    // one of the two sets is how `now` ends up listing an item whose `why` says
    // no such item exists.
    let mut matches: Vec<(String, Option<AttentionItem>, String)> = Vec::new();
    for project_id in ledger.project_ids()? {
        for include_dismissed in [true, false] {
            for (item, _) in visible(ledger, &project_id, &acks, include_dismissed)?.items {
                if item.id.starts_with(needle) {
                    matches.push((item.id.clone(), Some(item), project_id.clone()));
                }
            }
        }
    }
    // Ids that no longer derive but were dismissed: still answerable.
    let derived: std::collections::BTreeSet<&str> =
        matches.iter().map(|(id, _, _)| id.as_str()).collect();
    let expired: Vec<String> = acks
        .keys()
        .filter(|id| id.starts_with(needle) && !derived.contains(id.as_str()))
        .cloned()
        .collect();
    for id in expired {
        matches.push((id, None, String::new()));
    }

    matches.sort_by(|a, b| a.0.cmp(&b.0));
    matches.dedup_by(|a, b| a.0 == b.0);

    match matches.len() {
        0 => Ok(Resolution::NotFound),
        1 => {
            let (id, item, project_id) = matches.remove(0);
            let dismissed = acks.get(&id).copied();
            Ok(Resolution::One(Box::new(match item {
                Some(item) => view_of(ledger, &item, &project_id, dismissed)?,
                None => WhyView {
                    id,
                    project_id: None,
                    kind: None,
                    sentence: voice::no_longer_derives().to_string(),
                    action: voice::attention_action(AttentionKind::AllClear, None),
                    what_happened: voice::no_longer_derives().to_string(),
                    quoted: None,
                    receipts: Vec::new(),
                    records: Vec::new(),
                    dismissed_at_unix_ms: dismissed,
                    no_longer_derives: true,
                },
            })))
        }
        _ => Ok(Resolution::Ambiguous(
            matches.into_iter().map(|(id, _, _)| id).collect(),
        )),
    }
}

fn view_of(
    ledger: &Ledger,
    item: &AttentionItem,
    project_id: &str,
    dismissed: Option<i64>,
) -> Result<WhyView> {
    let mut receipts = Vec::new();
    if let Some(event_id) = item.anchor_event_id.as_deref() {
        if let Some(r) = receipt_for(ledger, event_id)? {
            receipts.push(r);
        }
    }
    let mut records = Vec::new();
    if !item.detail.is_empty() && item.kind != AttentionKind::Contradiction {
        records.push(item.detail.clone());
    }
    Ok(WhyView {
        id: item.id.clone(),
        project_id: Some(project_id.to_string()),
        kind: Some(item.kind),
        sentence: voice::attention_sentence(
            item.kind,
            item.claimed.as_deref(),
            item.observed.as_deref(),
            item.readiness,
        ),
        action: voice::attention_action(item.kind, item.readiness),
        what_happened: voice::what_happened(item.kind, item.readiness, receipts.len() as u64),
        // A contradiction's `detail` is the claim the project itself made.
        quoted: (item.kind == AttentionKind::Contradiction && !item.detail.is_empty())
            .then(|| item.detail.clone()),
        receipts,
        records,
        dismissed_at_unix_ms: dismissed,
        no_longer_derives: false,
    })
}

/// One indexed lookup by primary key — the receipt path back to a raw line.
fn receipt_for(ledger: &Ledger, event_id: &str) -> Result<Option<Receipt>> {
    let found = ledger
        .conn()
        .query_row(
            "SELECT id, session_id, raw_file, raw_line, raw_sha256, chain_hash
             FROM events WHERE id = ?1",
            params![event_id],
            |r| {
                Ok(Receipt {
                    event_id: r.get(0)?,
                    session_id: r.get(1)?,
                    raw_file: r.get(2)?,
                    raw_line: r.get(3)?,
                    raw_sha256: r.get(4)?,
                    chain_hash: r.get(5)?,
                })
            },
        )
        .ok();
    Ok(found)
}
