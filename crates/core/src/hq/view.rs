//! The thread view (everything one thread holds) and the session timeline.
//! Every string served here is masked by [`super::masked`] first.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::params_from_iter;
use serde::Serialize;
use serde_json::Value;

use super::{
    command_id, commands, edits, masked, prompts, tally, Index, Kind, Outcome, ThreadSummary,
    SCHEMA_VERSION,
};
use crate::error::Result;
use crate::extract::{Claim, Marker};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

#[derive(Debug, Clone, Serialize)]
pub struct PromptItem {
    pub event_id: String,
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    /// The whole prompt, masked. Never truncated.
    pub text: String,
    /// The same prompt split into what was typed and what was pasted
    /// (`kind: typed | pasted`), wrapper tags removed, each part masked.
    pub segments: Vec<super::prompts::Segment>,
    /// True when this prompt's turn did work (it may start or continue the
    /// thread); false for conversation.
    pub work: bool,
    pub version_id: Option<String>,
    pub raw_file: String,
    pub raw_line: i64,
    /// C34 pause: the row was timestamped inside a pause. A join against the
    /// pause record made when this is read; nothing is written. Absent — not
    /// `false` — when it was not, so a project that was never paused serves
    /// the bytes it served before.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub recorded_while_paused: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditItem {
    /// `<event_id>` or `<event_id>#<n>` when one call edited several files.
    pub id: String,
    pub event_id: String,
    pub session_id: String,
    pub ts: Option<String>,
    pub file: String,
    pub op: &'static str,
    pub action: Option<String>,
    pub moved_to: Option<String>,
    pub hunks: Vec<edits::Hunk>,
    pub lines_added: u64,
    pub lines_removed: u64,
    /// `true` the tool reported success; `false` it reported an error;
    /// `null` the record holds no result, or the edit ran inside a Codex
    /// program that failed somewhere (which call failed is not stated).
    pub ok: Option<bool>,
    /// `structured_patch` | `apply_patch` | `request` — see [`super::edits`].
    pub source: &'static str,
    /// `tool` (Claude Code), `codex_patch`, `codex_exec`.
    pub via: &'static str,
    pub version_id: Option<String>,
    pub raw_file: String,
    pub raw_line: i64,
    /// C34 pause: the row was timestamped inside a pause. A join against the
    /// pause record made when this is read; nothing is written. Absent — not
    /// `false` — when it was not, so a project that was never paused serves
    /// the bytes it served before.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub recorded_while_paused: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TallyItem {
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub errors: u64,
    pub unit: &'static str,
    pub runner: &'static str,
    pub lines: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandItem {
    pub id: String,
    pub event_id: String,
    pub part: usize,
    pub session_id: String,
    pub ts: Option<String>,
    /// The command, masked; empty when it could not be read (`readable`).
    pub command: String,
    pub readable: bool,
    pub outcome: Outcome,
    pub exit_code: Option<i64>,
    /// Up to 40 trailing lines of the output, masked.
    pub output_excerpt: Vec<String>,
    pub output_lines_total: u64,
    /// `command` — the output is this command's; `program` — the output is a
    /// whole Codex program's, which ran this command among others.
    pub output_scope: &'static str,
    pub is_test: bool,
    pub is_commit: bool,
    /// For a test run: the runner's own summary, summed (see [`super::tally`]).
    pub tally: Option<TallyItem>,
    /// The summary line the tally was read from (the last one), masked.
    pub tally_line: Option<String>,
    pub via: &'static str,
    pub version_id: Option<String>,
    pub raw_file: String,
    pub raw_line: i64,
    /// C34 pause: the row was timestamped inside a pause. A join against the
    /// pause record made when this is read; nothing is written. Absent — not
    /// `false` — when it was not, so a project that was never paused serves
    /// the bytes it served before.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub recorded_while_paused: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimItem {
    pub id: String,
    pub event_id: String,
    pub session_id: String,
    pub ts: Option<String>,
    /// The claim's sentence, masked.
    pub text: String,
    /// `tests_pass` | `builds` | `fixed` | `done` | `other`.
    pub kind: &'static str,
    /// True for kinds a run can verify (`tests_pass`, `builds`).
    pub has_verifier: bool,
    /// Which extractor rule fired (`ratio`, `outcome`, `completion`, …).
    pub marker: &'static str,
    /// The latest test command before this claim in the thread.
    pub rests_on_command_id: Option<String>,
    /// A passing test run follows this claim in the thread.
    pub followed_by_passing_test: bool,
    /// `proven` | `not_proven` | `contradicted` | `recorded` — the rule in
    /// [`super::statements`], the same one the statements route serves.
    pub verdict: super::statements::Verdict,
    /// Which branch of that rule decided it.
    pub verdict_basis: &'static str,
    pub version_id: Option<String>,
    pub raw_file: String,
    pub raw_line: i64,
    /// C34 pause: the row was timestamped inside a pause. A join against the
    /// pause record made when this is read; nothing is written. Absent — not
    /// `false` — when it was not, so a project that was never paused serves
    /// the bytes it served before.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub recorded_while_paused: bool,
    /// What people said was wrong with this statement (C30 R-LEDGER), oldest
    /// first, beside the verdict above and never in place of it. Absent — not
    /// empty — when nobody objected, so a thread with no objection serves the
    /// bytes it served before objections existed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objections: Vec<crate::statement_objection::ObjectionItem>,
}

/// One recorded assistant text message, including a sentence for which no
/// claim rule fired. It is a read-only projection of the existing event.
#[derive(Debug, Clone, Serialize)]
pub struct MessageItem {
    pub event_id: String,
    pub session_id: String,
    pub ts: Option<String>,
    pub text: String,
    pub version_id: Option<String>,
    pub raw_file: String,
    pub raw_line: i64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub recorded_while_paused: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionItem {
    pub id: String,
    pub commit: Option<String>,
    pub short: Option<String>,
    pub session_id: String,
    pub from_seq: i64,
    pub to_seq: i64,
    pub committed: bool,
    pub message: Option<String>,
    pub boundary_event_id: Option<String>,
    pub committed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionItem {
    pub id: String,
    pub ledger_project_id: String,
    pub source: String,
    /// `person` | `subagent`.
    pub origin: &'static str,
    pub started_at: Option<String>,
    pub last_at: Option<String>,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileTouch {
    pub path: String,
    pub edits: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRef {
    pub id: String,
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadView {
    pub schema_version: u32,
    pub project: ProjectRef,
    pub thread: ThreadSummary,
    pub prompts: Vec<PromptItem>,
    pub edits: Vec<EditItem>,
    pub commands: Vec<CommandItem>,
    pub claims: Vec<ClaimItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<MessageItem>,
    pub versions: Vec<VersionItem>,
    pub current_version_id: Option<String>,
    pub sessions: Vec<SessionItem>,
    pub files: Vec<FileTouch>,
    pub masking: MaskRecord,
}

struct Full {
    text: Option<String>,
    tool_input: Option<String>,
    tool_result: Option<String>,
    raw_file: String,
    raw_line: i64,
    is_error: bool,
}

fn load_full(ledger: &Ledger, ids: &[&str]) -> Result<HashMap<String, Full>> {
    let mut out = HashMap::new();
    for chunk in ids.chunks(400) {
        let marks = vec!["?"; chunk.len()].join(",");
        let mut stmt = ledger.conn().prepare(&format!(
            "SELECT id, text, tool_input, tool_result, raw_file, raw_line, is_error FROM events WHERE id IN ({marks})"
        ))?;
        let rows = stmt.query_map(params_from_iter(chunk.iter()), |r| {
            Ok((
                r.get::<_, String>(0)?,
                Full {
                    text: r.get(1)?,
                    tool_input: r.get(2)?,
                    tool_result: r.get(3)?,
                    raw_file: r.get(4)?,
                    raw_line: r.get(5)?,
                    is_error: r.get::<_, i64>(6)? != 0,
                },
            ))
        })?;
        for row in rows {
            let (id, full) = row?;
            out.insert(id, full);
        }
    }
    Ok(out)
}

/// Mask every line of a hunk. The hunk is masked as one text first, so a
/// secret that spans lines (an armoured key) is seen whole; when that changes
/// the number of lines (a block collapsed into one marker) the lines are
/// masked one by one instead, so each line keeps its kind.
fn mask_hunk(h: &edits::Hunk, record: &mut MaskRecord) -> edits::Hunk {
    let joined: Vec<&str> = h.lines.iter().map(|l| l.text.as_str()).collect();
    let mut whole = MaskRecord::default();
    let as_one = masked(&joined.join("\n"), &mut whole);
    let split: Vec<&str> = as_one.split('\n').collect();
    let texts: Vec<String> = if split.len() == h.lines.len() {
        record.merge(&whole);
        split.into_iter().map(str::to_string).collect()
    } else {
        // Each line on its own; the whole-text record is kept too, so the
        // collapsed block is still counted as masked.
        record.merge(&whole);
        h.lines.iter().map(|l| masked(&l.text, record)).collect()
    };
    edits::Hunk {
        lines: h
            .lines
            .iter()
            .zip(texts)
            .map(|(l, text)| edits::Line { kind: l.kind, text })
            .collect(),
        ..h.clone()
    }
}

/// [`super::prompts::segments`] of a prompt, each part masked.
fn mask_segments(text: &str, record: &mut MaskRecord) -> Vec<super::prompts::Segment> {
    super::prompts::segments(text)
        .into_iter()
        .map(|s| super::prompts::Segment {
            kind: s.kind,
            text: masked(&s.text, record),
        })
        .collect()
}

/// The trailing excerpt of an output (see `commands::EXCERPT_*`).
pub(crate) fn excerpt(text: &str, record: &mut MaskRecord) -> (Vec<String>, u64) {
    let all: Vec<&str> = text.lines().collect();
    let total = all.len() as u64;
    let window = &all[all.len().saturating_sub(commands::EXCERPT_MASK_WINDOW)..];
    // Masked before any cut (A209-C3), the window as one text.
    let (masked_window, rec) = crate::redact::mask_recorded(&window.join("\n"));
    record.merge(&rec);
    let lines: Vec<&str> = masked_window.lines().collect();
    let tail = &lines[lines.len().saturating_sub(commands::EXCERPT_LINES)..];
    let out = tail
        .iter()
        .map(|l| {
            if l.chars().count() <= commands::EXCERPT_LINE_CHARS {
                l.to_string()
            } else {
                let mut c =
                    crate::redact::truncate_respecting_markers(l, commands::EXCERPT_LINE_CHARS - 1);
                c.push('…');
                c
            }
        })
        .collect();
    (out, total)
}

/// Claim kind by phrase, first match wins (documented in API.md):
/// `tests_pass` — a ratio marker, or the sentence names tests/specs/suites and
/// passing/green/ok — and it does not report a failure (fail/red/broken/error
/// other than "0 failed"/"no errors"); `builds` — names a build/compile/clippy/lint/typecheck and
/// clean/success/pass/green/0 errors; `fixed` — says fixed/resolved; `done` —
/// a completion marker or done/complete/implemented/added/shipped/merged/
/// deployed; else `other`.
pub fn claim_kind(claim: &Claim) -> &'static str {
    let low = claim.text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| low.contains(w));
    // A sentence that reports failures is not a claim that tests pass.
    let reports_failure = (has(&["fail", " red", "broken", "error"])
        && !has(&["0 fail", "0 error", "no fail", "no error", "zero fail"]))
        || low.starts_with("red");
    if !reports_failure
        && (claim.marker == Marker::Ratio
            || (has(&["test", "spec", "suite"]) && has(&["pass", "green", " ok", "✓", "succeed"])))
    {
        return "tests_pass";
    }
    if has(&[
        "build",
        "compil",
        "clippy",
        "lint",
        "typecheck",
        "type-check",
        "tsc",
    ]) && has(&[
        "clean",
        "succe",
        "pass",
        "green",
        "0 error",
        "no error",
        "no warning",
        "✓",
    ]) {
        return "builds";
    }
    if has(&["fixed", "resolved"]) {
        return "fixed";
    }
    if claim.marker == Marker::Completion
        || has(&[
            "done",
            "complete",
            "implemented",
            "added",
            "shipped",
            "merged",
            "deployed",
        ])
    {
        return "done";
    }
    "other"
}

impl Index {
    /// One thread's view, or `None` when the project has no such thread.
    pub fn thread_view(
        &self,
        ledger: &Ledger,
        thread_id: &str,
        now_ms: i64,
    ) -> Result<Option<ThreadView>> {
        let Some(t) = self.threads.iter().find(|t| t.id == thread_id) else {
            return Ok(None);
        };
        let mut record = MaskRecord::default();
        let summary = self.summary(t, now_ms, &mut record);
        // C34 pause: the pauses recorded over this project's folder, read once,
        // so each row can be marked by its own time. A join; nothing is written.
        let marks = crate::capture_pause::PauseGate::read(ledger)?.marks_for(&self.project.root);
        let rows = &self.rows;
        let mut wanted: BTreeSet<&str> = BTreeSet::new();
        for &i in &t.rows {
            let r = &rows[i];
            if r.kind == Kind::ToolUse
                || r.prompt.is_some()
                || (r.kind == Kind::Assistant && r.text.is_some() && !r.copied)
                || self.claims.contains_key(&i)
            {
                wanted.insert(r.id.as_str());
                if let Some(ri) = r.result {
                    wanted.insert(rows[ri].id.as_str());
                }
            }
        }
        let ids: Vec<&str> = wanted.into_iter().collect();
        let full = load_full(ledger, &ids)?;
        let raw = |id: &str, record: &mut MaskRecord| -> (String, i64) {
            full.get(id)
                .map(|f| (masked(&f.raw_file, record), f.raw_line))
                .unwrap_or_default()
        };
        let version_id = |i: usize| self.versions.get(rows[i].version).map(|v| v.id.clone());
        let session_id = |i: usize| self.sessions[rows[i].session].id.clone();

        // Prompts.
        let mut prompt_rows: Vec<(usize, bool)> = t
            .work_prompts
            .iter()
            .map(|&i| (i, true))
            .chain(t.conversation.iter().map(|&i| (i, false)))
            .collect();
        prompt_rows.sort_by_key(|(i, _)| self.order_key(*i));
        let mut prompt_items = Vec::new();
        for (i, work) in prompt_rows {
            let r = &rows[i];
            let stored = full
                .get(r.id.as_str())
                .and_then(|f| f.text.clone())
                .unwrap_or_default();
            // A slash command's prompt is its `/name args`; anything else is
            // the stored text, whole.
            let text = match prompts::classify(&stored, false) {
                prompts::PromptClass::Human { text } => text,
                _ => r.prompt.clone().unwrap_or_default(),
            };
            let (raw_file, raw_line) = raw(&r.id, &mut record);
            let segments = mask_segments(&text, &mut record);
            prompt_items.push(PromptItem {
                recorded_while_paused: marks.marked(r.ts.as_deref()),
                event_id: r.id.clone(),
                session_id: session_id(i),
                seq: r.seq,
                ts: r.ts.clone(),
                segments,
                text: masked(&text, &mut record),
                work,
                version_id: version_id(i),
                raw_file,
                raw_line,
            });
        }

        // Edits and commands.
        let mut edit_items = Vec::new();
        let mut command_items = Vec::new();
        let mut touches: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
        let roots: Vec<String> = std::iter::once(self.project.root.clone())
            .chain(self.project.members.iter().map(|m| m.root.clone()))
            .collect();
        for &i in &t.rows {
            let r = &rows[i];
            if r.kind != Kind::ToolUse || (r.files.is_empty() && r.cmds.is_empty()) {
                continue;
            }
            let f = full.get(r.id.as_str());
            let input: Value = f
                .and_then(|f| f.tool_input.as_deref())
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            let result_row = r.result.map(|ri| &rows[ri]);
            let result: Option<Value> = r
                .result
                .and_then(|ri| full.get(rows[ri].id.as_str()))
                .and_then(|f| f.tool_result.as_deref())
                .and_then(|s| serde_json::from_str(s).ok());
            let (raw_file, raw_line) = raw(&r.id, &mut record);
            let tool = r.tool_name.as_deref().unwrap_or("");
            let codemode = tool == "exec";
            let file_edits: Vec<(edits::FileEdit, &'static str)> = if codemode {
                let program = input.get("input").and_then(Value::as_str).unwrap_or("");
                super::codemode::apply_patches(program)
                    .into_iter()
                    .filter_map(|c| c.value)
                    .flat_map(|p| edits::parse_patch(&p))
                    .map(|e| (e, "codex_exec"))
                    .collect()
            } else if edits::EDIT_TOOLS.contains(&tool) {
                let via =
                    if input.get("file_path").is_some() || input.get("notebook_path").is_some() {
                        "tool"
                    } else {
                        "codex_patch"
                    };
                edits::edits_of(tool, &input, result.as_ref())
                    .into_iter()
                    .map(|e| (e, via))
                    .collect()
            } else {
                Vec::new()
            };
            let result_error =
                result_row.map(|rr| full.get(rr.id.as_str()).is_some_and(|f| f.is_error));
            let ok = match result_error {
                None => None,
                Some(e) if codemode => (!e).then_some(true),
                Some(e) => Some(!e),
            };
            let many = file_edits.len() > 1;
            for (k, (e, via)) in file_edits.into_iter().enumerate() {
                let path = super::display_path(&e.path, &roots);
                let (added, removed) = (e.lines_added(), e.lines_removed());
                let entry = touches.entry(path.clone()).or_default();
                entry.0 += 1;
                entry.1 += added;
                entry.2 += removed;
                let hunks = e.hunks.iter().map(|h| mask_hunk(h, &mut record)).collect();
                edit_items.push(EditItem {
                    recorded_while_paused: marks.marked(r.ts.as_deref()),
                    id: if many || codemode {
                        format!("{}#{k}", r.id)
                    } else {
                        r.id.clone()
                    },
                    event_id: r.id.clone(),
                    session_id: session_id(i),
                    ts: r.ts.clone(),
                    file: masked(&path, &mut record),
                    op: e.op,
                    action: e.action.clone(),
                    moved_to: e
                        .moved_to
                        .as_deref()
                        .map(|m| masked(&super::display_path(m, &roots), &mut record)),
                    hunks,
                    lines_added: added,
                    lines_removed: removed,
                    ok,
                    source: e.source,
                    via,
                    version_id: version_id(i),
                    raw_file: raw_file.clone(),
                    raw_line,
                });
            }
            let output = result
                .as_ref()
                .map(commands::result_text)
                .unwrap_or_default();
            for c in &r.cmds {
                let outcome = self.outcome(i, c);
                let exit_code = if codemode && r.cmds.len() != 1 {
                    None
                } else {
                    commands::exit_code(&output)
                };
                let (excerpt_lines, total) = if result.is_some() {
                    excerpt(&output, &mut record)
                } else {
                    (Vec::new(), 0)
                };
                let t_ = if c.test { tally::tally(&output) } else { None };
                command_items.push(CommandItem {
                    recorded_while_paused: marks.marked(r.ts.as_deref()),
                    id: command_id(&r.id, c),
                    event_id: r.id.clone(),
                    part: c.part,
                    session_id: session_id(i),
                    ts: r.ts.clone(),
                    command: masked(c.text.as_deref().unwrap_or(""), &mut record),
                    readable: c.text.is_some(),
                    outcome,
                    exit_code,
                    output_excerpt: excerpt_lines,
                    output_lines_total: total,
                    output_scope: if codemode && r.cmds.len() != 1 {
                        "program"
                    } else {
                        "command"
                    },
                    is_test: c.test,
                    is_commit: c.commit,
                    tally_line: t_.as_ref().map(|t| masked(&t.line, &mut record)),
                    tally: t_.map(|t| TallyItem {
                        passed: t.passed,
                        failed: t.failed,
                        skipped: t.skipped,
                        errors: t.errors,
                        unit: t.unit,
                        runner: t.runner,
                        lines: t.lines,
                    }),
                    via: if codemode { "codex_exec" } else { "bash" },
                    version_id: version_id(i),
                    raw_file: raw_file.clone(),
                    raw_line,
                });
            }
        }

        // Claims.
        let positions: HashMap<usize, usize> =
            t.rows.iter().enumerate().map(|(p, &i)| (i, p)).collect();
        let tests_at: Vec<(usize, String, Outcome)> = t
            .rows
            .iter()
            .enumerate()
            .flat_map(|(p, &i)| {
                rows[i]
                    .cmds
                    .iter()
                    .filter(|c| c.test)
                    .map(move |c| (p, command_id(&rows[i].id, c), self.outcome(i, c)))
            })
            .collect();
        let verdicts: HashMap<(usize, usize), super::statements::Judged> =
            super::statements::judge_thread(self, t)
                .into_iter()
                .collect();
        // C30 R-LEDGER: what people said was wrong, from another table. Read
        // here and attached to the claim beside its verdict; nothing above or
        // below reads it, so no verdict, count or `whose_move` can move.
        let objected = crate::statement_objection::standing(ledger)?;
        let mut claim_items = Vec::new();
        for &i in &t.rows {
            let Some(found) = self.claims.get(&i) else {
                continue;
            };
            let pos = positions.get(&i).copied().unwrap_or(0);
            let rests_on = tests_at
                .iter()
                .rev()
                .find(|(p, _, _)| *p < pos)
                .map(|(_, id, _)| id.clone());
            let followed = tests_at
                .iter()
                .any(|(p, _, o)| *p > pos && *o == Outcome::Succeeded);
            let (raw_file, raw_line) = raw(&rows[i].id, &mut record);
            for (n, c) in found.iter().enumerate() {
                let kind = claim_kind(c);
                let judged =
                    verdicts
                        .get(&(i, n))
                        .copied()
                        .unwrap_or(super::statements::Judged::of(
                            super::statements::Verdict::Recorded,
                            "no_verifier",
                        ));
                let id = format!("{}#{n}", rows[i].id);
                let objections = objected
                    .get(&id)
                    .map(|kept| {
                        kept.iter()
                            .map(|o| o.served(&c.text, &mut record))
                            .collect()
                    })
                    .unwrap_or_default();
                claim_items.push(ClaimItem {
                    recorded_while_paused: marks.marked(rows[i].ts.as_deref()),
                    id,
                    event_id: rows[i].id.clone(),
                    session_id: session_id(i),
                    ts: rows[i].ts.clone(),
                    text: masked(&c.text, &mut record),
                    kind,
                    has_verifier: matches!(kind, "tests_pass" | "builds"),
                    marker: c.marker.as_str(),
                    rests_on_command_id: rests_on.clone(),
                    followed_by_passing_test: followed,
                    verdict: judged.verdict,
                    verdict_basis: judged.basis,
                    version_id: version_id(i),
                    raw_file: raw_file.clone(),
                    raw_line,
                    objections,
                });
            }
        }

        // Every assistant sentence is readable in the record, even if no
        // statement rule classified it as a claim. Keep the event identity so
        // Every statement can list such sentences under Also said without
        // counting them as proof.
        let mut message_items = Vec::new();
        for &i in &t.rows {
            let r = &rows[i];
            if r.kind != Kind::Assistant || r.copied {
                continue;
            }
            let Some(text) = r.text.as_ref().filter(|text| !text.trim().is_empty()) else {
                continue;
            };
            let (raw_file, raw_line) = raw(&r.id, &mut record);
            message_items.push(MessageItem {
                recorded_while_paused: marks.marked(r.ts.as_deref()),
                event_id: r.id.clone(),
                session_id: session_id(i),
                ts: r.ts.clone(),
                text: masked(text, &mut record),
                version_id: version_id(i),
                raw_file,
                raw_line,
            });
        }

        // Versions in thread order.
        let mut seen = BTreeSet::new();
        let mut versions = Vec::new();
        for &i in &t.rows {
            let v = rows[i].version;
            if !seen.insert(v) {
                continue;
            }
            let Some(ver) = self.versions.get(v) else {
                continue;
            };
            versions.push(VersionItem {
                id: ver.id.clone(),
                commit: ver.commit.clone(),
                short: ver.commit.as_ref().map(|c| c.chars().take(7).collect()),
                session_id: self.sessions[ver.session].id.clone(),
                from_seq: ver.from_seq,
                to_seq: ver.to_seq,
                committed: ver.committed,
                message: ver.message.as_deref().map(|m| masked(m, &mut record)),
                boundary_event_id: ver.boundary.map(|b| rows[b].id.clone()),
                committed_at: ver.boundary.and_then(|b| rows[b].ts.clone()),
            });
        }

        let sessions = t
            .sessions
            .iter()
            .map(|&s| {
                let info = &self.sessions[s];
                SessionItem {
                    id: info.id.clone(),
                    ledger_project_id: info.ledger_project.clone(),
                    source: info.source.clone(),
                    origin: if info.origin.subagent {
                        "subagent"
                    } else {
                        "person"
                    },
                    started_at: info.first_ts.clone(),
                    last_at: info.last_ts.clone(),
                    file: masked(&info.file_path, &mut record),
                }
            })
            .collect();
        let files = touches
            .into_iter()
            .map(|(path, (n, a, r))| FileTouch {
                path: masked(&path, &mut record),
                edits: n,
                lines_added: a,
                lines_removed: r,
            })
            .collect();
        Ok(Some(ThreadView {
            schema_version: SCHEMA_VERSION,
            project: ProjectRef {
                id: self.project.id.clone(),
                name: masked(&self.project.name, &mut record),
                root: masked(&self.project.root, &mut record),
            },
            current_version_id: summary.current_version_id.clone(),
            thread: summary,
            prompts: prompt_items,
            edits: edit_items,
            commands: command_items,
            claims: claim_items,
            messages: message_items,
            versions,
            sessions,
            files,
            masking: record,
        }))
    }
}

/// One thread of one HQ project.
pub fn thread(
    ledger: &Ledger,
    project_id: &str,
    thread_id: &str,
    now_ms: i64,
) -> Result<Option<ThreadView>> {
    let Some(p) = super::find_project(ledger, project_id)? else {
        return Ok(None);
    };
    super::index(ledger, &p)?.thread_view(ledger, thread_id, now_ms)
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDetail {
    pub name: String,
    pub source_name: Option<String>,
    /// Commands, or `file (+a −r)` lines for edits, or the masked input
    /// (≤ 2,000 characters) for any other tool.
    pub summary: Vec<String>,
    pub outcome: Outcome,
    pub exit_code: Option<i64>,
    pub output_excerpt: Vec<String>,
    pub output_lines_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineItem {
    pub event_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    /// `prompt` | `agent_prompt` | `injected` | `assistant` | `thinking` |
    /// `tool` | `tool_result` | `summary`.
    pub kind: &'static str,
    pub text: Option<String>,
    /// For `prompt` and `agent_prompt`: typed and pasted parts (see
    /// [`PromptItem::segments`]); absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<super::prompts::Segment>>,
    pub tool: Option<ToolDetail>,
    pub sidechain: bool,
    pub raw_file: String,
    pub raw_line: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionTimeline {
    pub schema_version: u32,
    pub session_id: String,
    pub ledger_project_id: String,
    pub hq_project_id: Option<String>,
    pub source: String,
    /// `person` | `subagent`.
    pub origin: &'static str,
    pub file: String,
    pub items: Vec<TimelineItem>,
    pub masking: MaskRecord,
}

/// The full ordered timeline of one session (everything but `meta` rows),
/// masked. `None` when the session is not in the ledger.
pub fn session(ledger: &Ledger, session_id: &str) -> Result<Option<SessionTimeline>> {
    let conn = ledger.conn();
    let found: Option<(String, String, String)> = conn
        .query_row(
            "SELECT s.project_id, s.file_path, p.source FROM sessions s
             JOIN projects p ON p.id = s.project_id WHERE s.id = ?1",
            [session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((ledger_project, file_path, source)) = found else {
        return Ok(None);
    };
    let subagent = source == "codex"
        && super::codex_origin(std::path::Path::new(&file_path)).is_some_and(|o| o.subagent);
    let hq_project_id = super::hq_projects(ledger)?
        .into_iter()
        .find(|p| {
            p.members
                .iter()
                .any(|m| m.ledger_project_id == ledger_project)
        })
        .map(|p| p.id);
    let mut stmt = conn.prepare(
        "SELECT id, seq, ts, kind, text, tool_name, source_tool_name, tool_use_id, tool_input,
                tool_result, is_error, is_sidechain, raw_file, raw_line
         FROM events WHERE session_id = ?1 AND kind <> 'meta' ORDER BY seq",
    )?;
    type R = (
        String,
        i64,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        bool,
        bool,
        String,
        i64,
    );
    let rows: Vec<R> = stmt
        .query_map([session_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get::<_, i64>(10)? != 0,
                r.get::<_, i64>(11)? != 0,
                r.get(12)?,
                r.get(13)?,
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut results: HashMap<String, usize> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        if r.3 == "tool_result" {
            if let Some(t) = &r.7 {
                results.entry(t.clone()).or_insert(i);
            }
        }
    }
    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut record = MaskRecord::default();
    let mut items = Vec::new();
    let mut wrapper_at: Option<i64> = None;
    for (i, r) in rows.iter().enumerate() {
        let (
            id,
            seq,
            ts,
            kind,
            text,
            tool_name,
            source_name,
            tool_use_id,
            input,
            _,
            _,
            sidechain,
            raw_file,
            raw_line,
        ) = r;
        let mut item = TimelineItem {
            event_id: id.clone(),
            seq: *seq,
            ts: ts.clone(),
            kind: "summary",
            text: None,
            segments: None,
            tool: None,
            sidechain: *sidechain,
            raw_file: masked(raw_file, &mut record),
            raw_line: *raw_line,
        };
        match kind.as_str() {
            "user_msg" => {
                let t = text.clone().unwrap_or_default();
                let at = ts.as_deref().and_then(super::parse_ms).unwrap_or(0);
                let after_wrapper =
                    wrapper_at.is_some_and(|w| (at - w).abs() <= super::WRAPPER_BODY_MS);
                let class = prompts::classify(&t, after_wrapper);
                wrapper_at = (class == prompts::PromptClass::CommandWrapper).then_some(at);
                item.kind = match class {
                    prompts::PromptClass::Human { .. } if subagent || *sidechain => "agent_prompt",
                    prompts::PromptClass::Human { .. } => "prompt",
                    _ => "injected",
                };
                if item.kind != "injected" {
                    item.segments = Some(mask_segments(&t, &mut record));
                }
                item.text = Some(masked(&t, &mut record));
            }
            "assistant_text" | "assistant_thinking" | "summary" => {
                wrapper_at = None;
                item.kind = match kind.as_str() {
                    "assistant_text" => "assistant",
                    "assistant_thinking" => "thinking",
                    _ => "summary",
                };
                item.text = text.as_deref().map(|t| masked(t, &mut record));
            }
            "tool_use" => {
                wrapper_at = None;
                item.kind = "tool";
                let name = tool_name.clone().unwrap_or_default();
                let v: Value = input
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(Value::Null);
                let res_i = tool_use_id.as_ref().and_then(|t| results.get(t)).copied();
                if let Some(ri) = res_i {
                    used.insert(ri);
                }
                let result: Option<Value> = res_i
                    .and_then(|ri| rows[ri].9.as_deref())
                    .and_then(|s| serde_json::from_str(s).ok());
                let is_error = res_i.is_some_and(|ri| rows[ri].10);
                let output = result
                    .as_ref()
                    .map(commands::result_text)
                    .unwrap_or_default();
                let mut summary = Vec::new();
                let mut n_cmds = 0usize;
                if name == "Bash" {
                    if let Some(c) = commands::command_of(&v) {
                        summary.push(masked(&c, &mut record));
                        n_cmds = 1;
                    }
                } else if name == "exec" {
                    let program = v.get("input").and_then(Value::as_str).unwrap_or("");
                    for c in super::codemode::exec_commands(program) {
                        n_cmds += 1;
                        summary.push(masked(
                            c.value.as_deref().unwrap_or("(command not readable)"),
                            &mut record,
                        ));
                    }
                    for p in super::codemode::apply_patches(program)
                        .into_iter()
                        .filter_map(|c| c.value)
                    {
                        for e in edits::parse_patch(&p) {
                            summary.push(masked(
                                &format!(
                                    "{} (+{} −{})",
                                    e.path,
                                    e.lines_added(),
                                    e.lines_removed()
                                ),
                                &mut record,
                            ));
                        }
                    }
                    if summary.is_empty() {
                        summary.push(clip(program, 2000, &mut record));
                    }
                } else if edits::EDIT_TOOLS.contains(&name.as_str()) {
                    for e in edits::edits_of(&name, &v, result.as_ref()) {
                        summary.push(masked(
                            &format!("{} (+{} −{})", e.path, e.lines_added(), e.lines_removed()),
                            &mut record,
                        ));
                    }
                } else {
                    summary.push(clip(input.as_deref().unwrap_or(""), 2000, &mut record));
                }
                let outcome = match res_i {
                    None => Outcome::Unknown,
                    Some(_) if name == "exec" => match (n_cmds, commands::exit_code(&output)) {
                        (1, Some(0)) => Outcome::Succeeded,
                        (1, Some(_)) => Outcome::Failed,
                        _ if is_error => Outcome::Failed,
                        _ => Outcome::Succeeded,
                    },
                    Some(_) if is_error => Outcome::Failed,
                    Some(_) => Outcome::Succeeded,
                };
                let (excerpt_lines, total) = if result.is_some() {
                    excerpt(&output, &mut record)
                } else {
                    (Vec::new(), 0)
                };
                item.tool = Some(ToolDetail {
                    name,
                    source_name: source_name.clone(),
                    summary,
                    outcome,
                    exit_code: commands::exit_code(&output),
                    output_excerpt: excerpt_lines,
                    output_lines_total: total,
                });
            }
            _ => {
                // tool_result: shown on its call; an orphan is shown alone.
                if used.contains(&i)
                    || results
                        .get(tool_use_id.as_deref().unwrap_or(""))
                        .is_some_and(|&ri| ri != i)
                {
                    continue;
                }
                item.kind = "tool_result";
                let output =
                    r.9.as_deref()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .map(|v| commands::result_text(&v))
                        .unwrap_or_default();
                item.text = Some(excerpt(&output, &mut record).0.join("\n"));
            }
        }
        items.push(item);
    }
    Ok(Some(SessionTimeline {
        schema_version: SCHEMA_VERSION,
        session_id: session_id.to_string(),
        ledger_project_id: ledger_project,
        hq_project_id,
        source,
        origin: if subagent { "subagent" } else { "person" },
        file: masked(&file_path, &mut record),
        items,
        masking: record,
    }))
}

/// Mask `raw`, then cut it to `max` characters with `…`, never inside a
/// `[masked: …]` marker.
fn clip(raw: &str, max: usize, record: &mut MaskRecord) -> String {
    let (text, rec) = crate::redact::mask_recorded(raw);
    record.merge(&rec);
    if text.chars().count() <= max {
        return text;
    }
    let mut s = crate::redact::truncate_respecting_markers(&text, max - 1);
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::ClaimKind;

    fn claim(text: &str, marker: Marker) -> Claim {
        Claim {
            text: text.into(),
            span_start: 0,
            span_end: 0,
            kind: ClaimKind::Marker,
            marker,
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn claim_kinds_separate_verifiable_from_not() {
        assert_eq!(
            claim_kind(&claim("104/104 green", Marker::Ratio)),
            "tests_pass"
        );
        assert_eq!(
            claim_kind(&claim("All 54 proxy tests pass.", Marker::Outcome)),
            "tests_pass"
        );
        assert_eq!(
            claim_kind(&claim("Clippy clean, 0 warnings.", Marker::Outcome)),
            "builds"
        );
        assert_eq!(
            claim_kind(&claim(
                "I fixed the parser in `a.rs`.",
                Marker::FirstPersonPast
            )),
            "fixed"
        );
        assert_eq!(
            claim_kind(&claim("Deployed `v2`.", Marker::Completion)),
            "done"
        );
        assert_eq!(
            claim_kind(&claim("I set `x`.", Marker::FirstPersonPast)),
            "other"
        );
    }

    #[test]
    fn an_excerpt_is_the_tail_and_masked() {
        let text: String = (0..100).map(|i| format!("line {i}\n")).collect();
        let mut rec = MaskRecord::default();
        let (lines, total) = excerpt(&text, &mut rec);
        assert_eq!(total, 100);
        assert_eq!(lines.len(), commands::EXCERPT_LINES);
        assert_eq!(lines.last().map(String::as_str), Some("line 99"));
        let (lines, _) = excerpt("token=ghp_0123456789abcdefghijABCDEFGHIJ012345", &mut rec);
        assert!(!lines[0].contains("ghp_0123456789"), "{lines:?}");
    }
}
