//! §10 question 1 — what happened, and question 6 — risk flags.

use serde::{Deserialize, Serialize};

/// One line of the timeline.
///
/// `tool` and `input` are ADDITIVE (`thruline.explain` stays at schema_version
/// 1). Both are `Option` and both skip serialization when absent, exactly as
/// `risk` already does, so a row for a non-tool event is byte-identical to what
/// this type emitted before them and a consumer reading `{seq, kind, text}`
/// keeps working. `#[serde(default)]` makes the reverse true as well: an older
/// document still deserializes into this newer struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub kind: String,
    /// **Do not widen this field.** It is the flattened `"{tool} {first 50
    /// characters of the input}…"` line, and every renderer downstream — the
    /// terminal, `--md`, the HTML report, `thruline tui`, and two string
    /// splitters in the UI — parses it as it is. Widening it looks like the
    /// same edit as adding `tool`/`input` below and has a completely different
    /// blast radius. The structured fields exist so nobody has to.
    pub text: String,
    /// Commands and tool calls that deserve a second look (§10 item 6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    /// `payload.tool`, verbatim — a bare harness identifier, never cut, so it
    /// registers no truncation site. It ends the guessing: a consumer that
    /// wanted the tool name previously had to re-split `text` on a space or on
    /// `" {"`, and two consumers doing it two ways disagreed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The tool's argument object as the ledger holds it — already masked and
    /// already bounded to 2,000 characters at capture — re-masked and re-bounded
    /// on the way out. `text` cuts this to 50 characters, which is short enough
    /// that hundreds of distinct commands render as one string; this is the
    /// field that carries what was cut off.
    ///
    /// It is the whole argument object, not one hand-picked key, and in
    /// particular the agent-written `description` is *inside* it rather than
    /// beside it. That is deliberate: the gap between what the agent said it was
    /// doing and what actually ran is the artefact this product audits, so the
    /// agent's own words must never be promoted to a sibling of `tool` where a
    /// renderer could mistake them for a headline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
}

/// The compact timeline §10 asks for, plus its risk flags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    pub entries: Vec<Entry>,
    pub risks: Vec<Entry>,
    /// Tool calls collapsed into a run, so a 40-edit session is readable.
    pub grouped: usize,
}

/// Commands §10 item 6 names, plus the ones with the same blast radius.
const DESTRUCTIVE: &[&str] = &[
    "rm -rf",
    "rm -fr",
    "git reset --hard",
    "git clean -fd",
    "git push --force",
    "git push -f",
    "drop table",
    "drop database",
    "truncate table",
    "chmod -r 777",
    "mkfs",
    "dd if=",
    ">/dev/sd",
];

/// Build a timeline from event views.
///
/// Consecutive tool calls on the same tool collapse into one line: §10 wants
/// sixty lines, and a session with forty edits has forty otherwise.
///
/// **What "the same" means changed when `input` arrived.** The run key used to
/// be the rendered `text` alone — a 50-character cut — so two genuinely
/// different commands that happened to share a 50-character prologue were
/// folded into one line reading `(x2)`, which asserted they were the same call.
/// That was survivable only while nothing downstream could tell; a folded entry
/// now carries an `input`, and carrying one command's arguments on a line that
/// counts two is a false receipt. The key is therefore the full identity —
/// text, tool and input together — so `(xN)` means N identical calls and
/// nothing distinct is folded out of reach.
pub fn build(events: &[TimelineEvent]) -> Timeline {
    let mut out = Timeline::default();
    let mut last_call: Option<(String, Option<String>, Option<String>)> = None;
    let mut run = 0usize;

    for e in events {
        let risk = risk_of(e);
        if let Some(reason) = &risk {
            out.risks.push(Entry {
                seq: e.seq,
                kind: e.kind.clone(),
                text: e.text.clone(),
                risk: Some(reason.clone()),
                tool: e.tool.clone(),
                input: e.input.clone(),
            });
        }
        // Group consecutive identical calls, but never group anything risky —
        // the whole point of a risk flag is that it is visible.
        if e.kind == "tool_call" && risk.is_none() {
            let key = (e.text.clone(), e.tool.clone(), e.input.clone());
            if last_call.as_ref() == Some(&key) {
                run += 1;
                out.grouped += 1;
                if let Some(entry) = out.entries.last_mut() {
                    entry.text = format!("{} (x{})", e.text, run + 1);
                }
                continue;
            }
            last_call = Some(key);
            run = 0;
        } else {
            last_call = None;
            run = 0;
        }
        out.entries.push(Entry {
            seq: e.seq,
            kind: e.kind.clone(),
            text: e.text.clone(),
            risk,
            tool: e.tool.clone(),
            input: e.input.clone(),
        });
    }
    out
}

/// An event, reduced to what the timeline reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineEvent {
    pub seq: u64,
    pub kind: String,
    pub text: String,
    /// The argv of an `exec`, when this is one. Risk is judged on argv, never
    /// on a rendered command string (§13).
    pub argv: Vec<String>,
    /// A Bash tool's command, when the harness reported one. Risk reads this
    /// WHOLE, before any cut — so no bound placed on `input` below can hide a
    /// destructive command from the risk list.
    pub command: Option<String>,
    /// The harness's name for the tool, carried through to [`Entry::tool`].
    pub tool: Option<String>,
    /// The masked, bounded argument object, carried through to
    /// [`Entry::input`]. The caller owns the mask-then-cut; this type only
    /// transports the result.
    pub input: Option<String>,
}

fn risk_of(e: &TimelineEvent) -> Option<String> {
    let haystack = if e.argv.is_empty() {
        e.command.clone().unwrap_or_default()
    } else {
        e.argv.join(" ")
    }
    .to_ascii_lowercase();
    if haystack.is_empty() {
        return None;
    }
    DESTRUCTIVE
        .iter()
        .find(|d| haystack.contains(*d))
        .map(|d| format!("destructive command: `{d}`"))
}
