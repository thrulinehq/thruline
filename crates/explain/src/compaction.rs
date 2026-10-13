//! §10 question 3 — what it forgot across a compaction.

use serde::{Deserialize, Serialize};

/// Something that was open before a compaction and never mentioned after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LostItem {
    pub what: String,
    pub kind: String,
}

/// The §10 compaction-loss report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionLoss {
    pub compactions: usize,
    pub lost: Vec<LostItem>,
    /// Recorded when a `compaction_pre` carried nothing to compare against —
    /// the report is then empty because nothing was captured, which is not the
    /// same as nothing being lost.
    pub unmeasurable: Option<String>,
}

/// A `compaction_pre` payload, reduced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreCompaction {
    pub seq: u64,
    pub open_promises: Vec<String>,
    pub open_tasks: Vec<String>,
    pub recent_claims: Vec<String>,
}

/// Diff what was open before each compaction against what happened after it.
///
/// `after_text` is every message and tool input that followed, concatenated —
/// an item is "lost" when nothing after the compaction mentions it again.
pub fn compaction_loss(pres: &[PreCompaction], after_text: &[(u64, String)]) -> CompactionLoss {
    let mut out = CompactionLoss {
        compactions: pres.len(),
        ..CompactionLoss::default()
    };
    if pres.is_empty() {
        return out;
    }
    let empty = pres.iter().all(|p| {
        p.open_promises.is_empty() && p.open_tasks.is_empty() && p.recent_claims.is_empty()
    });
    if empty {
        out.unmeasurable = Some(
            "the compaction events carried no open promises, tasks or claims, so there \
             is nothing to compare against — the report is empty because nothing was \
             captured, not because nothing was lost"
                .to_string(),
        );
        return out;
    }

    for pre in pres {
        let after: String = after_text
            .iter()
            .filter(|(seq, _)| *seq > pre.seq)
            .map(|(_, t)| t.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        for (items, kind) in [
            (&pre.open_promises, "promise"),
            (&pre.open_tasks, "task"),
            (&pre.recent_claims, "claim"),
        ] {
            for item in items {
                if !mentioned(&after, item) {
                    out.lost.push(LostItem {
                        what: item.clone(),
                        kind: kind.to_string(),
                    });
                }
            }
        }
    }
    out
}

/// Is this item referred to again?
///
/// Matched on the item's distinctive words rather than on the whole string: an
/// agent restates a promise in its own words, and requiring a verbatim repeat
/// would report everything as lost.
fn mentioned(after: &str, item: &str) -> bool {
    let words: Vec<String> = item
        .to_ascii_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| w.len() > 4)
        .collect();
    if words.is_empty() {
        return after.contains(&item.to_ascii_lowercase());
    }
    let hits = words.iter().filter(|w| after.contains(w.as_str())).count();
    // Half the distinctive words is the threshold. Stated as a number because
    // it is a precision choice, not a fact: too low and nothing is ever lost,
    // too high and every restatement is.
    hits * 2 >= words.len()
}
