//! Record search: what ⌘K's Find rows find (C30 R-ROUTES "Record search").
//!
//! One project's record, across every thread (Change) it holds, searched by
//! the words a person types. Each thing found is one of the kinds the design
//! draws for a Find row (DESIGN-SPEC §3.08):
//!
//! | kind      | what it is                                   | grouped by              |
//! |-----------|----------------------------------------------|-------------------------|
//! | `Request` | a person's prompt                            | one per prompt          |
//! | `Said`    | a statement the agent made (a claim)         | one per claim           |
//! | `File`    | a file the agent edited                      | thread and path         |
//! | `Read`    | a file the agent read (Claude Code `Read`)   | thread and path         |
//! | `Run`     | a test command the agent ran                 | thread and command text |
//! | `Command` | any other command the agent ran              | thread and command text |
//! | `Version` | a version of the code the record holds       | one per version         |
//!
//! A grouped hit carries `count` (how many times) and the latest one's
//! instant and event id. Prompts, edits, commands, claims and versions are the
//! rows the thread view serves (same index, same rules); a `Read` is a
//! `tool_use` row named `Read` inside a thread, whose `file_path` is read from
//! the event by primary key once per index. A copied row (a resumed
//! transcript's history) is never a thread member, so it is never found
//! twice.
//!
//! # The match rule
//!
//! The palette's own rule (`Command-Ask/logic.js`): the typed text is
//! trimmed and lower-cased; its **words** are the runs of `[a-z0-9/._-]` at
//! least four characters long that are not in [`STOP_WORDS`]. A thing matches
//! when its searched text, lower-cased, contains the whole typed text **or**
//! any one word. The searched text is the **masked** text, the text a person
//! would be shown, so a secret the masker removed can never be found by
//! typing it.
//!
//! # What it serves
//!
//! Every string has been through [`crate::redact::mask_recorded`] (through
//! [`super::masked`]); `masking` is the merged record of everything served.
//! A version's `id`, `short` and `commit` are identifiers ThruLine derives,
//! served as the thread view serves them. Read-only and deterministic: the
//! same index, the same rows and the same query give the same answer.
//!
//! # Cost
//!
//! [`Corpus::build`] masks every searchable string once and reads each
//! `Read` call's path by primary key (`events.id`), in chunks of 400. The
//! caller keeps the corpus beside the index it was built from, so a search is
//! one linear pass of `contains` over the corpus in memory, then a sort of
//! what matched and a window of at most [`TEXT_WINDOW_CHARS`] around the match
//! for each of the at most [`MAX_LIMIT`] hits served.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{params_from_iter, Connection};
use serde::Serialize;

use super::statements::{judge_thread, Verdict};
use super::view::claim_kind;
use super::{commands, display_path, masked, tally, Index, Kind, Outcome};
use crate::capture_exclusion::{marker as exclusion_marker, ExclusionPolicy};
use crate::error::Result;
use crate::ledger::Ledger;
use crate::redact::{truncate_respecting_markers, MaskRecord};

/// Bump when a served field changes meaning or disappears.
pub const SCHEMA_VERSION: u32 = 1;
/// How many hits a search serves when the caller names no limit.
pub const DEFAULT_LIMIT: usize = 20;
/// The most hits one search serves.
pub const MAX_LIMIT: usize = 100;
/// The longest typed text a search accepts, in characters.
pub const MAX_QUERY_CHARS: usize = 200;
/// A served text longer than this is cut to a window around the match.
pub const TEXT_WINDOW_CHARS: usize = 160;
/// The tools whose calls are a `Read`: Claude Code's `Read`.
pub const READ_TOOLS: &[&str] = &["Read"];
/// Words the palette does not search on their own (`Command-Ask/logic.js`).
pub const STOP_WORDS: &[&str] = &[
    "what",
    "which",
    "when",
    "where",
    "were",
    "there",
    "that",
    "this",
    "with",
    "from",
    "have",
    "does",
    "about",
    "should",
    "would",
    "could",
    "anything",
    "something",
    "after",
    "before",
    "into",
    "your",
    "they",
    "them",
    "then",
    "than",
    "been",
    "being",
    "will",
    "just",
    "some",
    "ever",
];

/// The kind of thing found, named as the design's Find row tags it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum FindKind {
    Request,
    Said,
    File,
    Read,
    Run,
    Command,
    Version,
}

impl FindKind {
    /// Every kind, in the order ties are broken.
    pub const ALL: [FindKind; 7] = [
        FindKind::Request,
        FindKind::Said,
        FindKind::File,
        FindKind::Read,
        FindKind::Run,
        FindKind::Command,
        FindKind::Version,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FindKind::Request => "Request",
            FindKind::Said => "Said",
            FindKind::File => "File",
            FindKind::Read => "Read",
            FindKind::Run => "Run",
            FindKind::Command => "Command",
            FindKind::Version => "Version",
        }
    }

    /// The kinds the open Change's own thread view already holds, each with
    /// its step there. Of the thread a caller names as open, these are not
    /// served: the palette finds them in that view, so a step it names is
    /// the step every Change screen names. `Read` and `Version` are.
    pub fn in_thread_view(self) -> bool {
        matches!(
            self,
            FindKind::Request | FindKind::Said | FindKind::File | FindKind::Run | FindKind::Command
        )
    }
}

/// A test run's own summary, as the thread view's `tally` counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HitTally {
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub errors: u64,
}

/// A `Version` hit's identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HitVersion {
    pub id: String,
    pub short: Option<String>,
    pub commit: Option<String>,
    pub committed: bool,
    /// The commit message, masked.
    pub message: Option<String>,
    /// It is the version of its thread's last row.
    pub current: bool,
}

/// One thing found. Every field is present on every hit; a field that does
/// not belong to the hit's kind is `null`.
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub kind: FindKind,
    /// The thread (Change) it belongs to; its Change screens are addressed
    /// by this id.
    pub thread_id: String,
    /// That thread's title, by the Inbox's own rule, masked; `""` when its
    /// prompts hold no sentence.
    pub thread_title: String,
    /// It belongs to the thread the caller named as open.
    pub open_thread: bool,
    /// What a person reads, masked; a window around the match when longer
    /// than [`TEXT_WINDOW_CHARS`], with `…` where it was cut.
    pub text: String,
    /// The latest occurrence's instant, as recorded.
    pub at: Option<String>,
    /// The latest occurrence's record: an `events.id`, or `<event_id>#<n>`
    /// for a statement (its index in that message).
    pub event_id: String,
    /// How many times (grouped kinds); 1 for the others.
    pub count: u64,
    /// The recording's source: `claude_code` | `codex`.
    pub source: String,
    /// `Request`: the prompt's turn did work.
    pub work: Option<bool>,
    /// `Run` / `Command`: how the latest one ended.
    pub outcome: Option<Outcome>,
    /// `Run`: the exit code its output states, when it states one.
    pub exit_code: Option<i64>,
    /// `Run`: the runner's summary, when the output has one.
    pub tally: Option<HitTally>,
    /// `Said`: the verdict the statements route serves for it.
    pub verdict: Option<Verdict>,
    /// `Said`: `tests_pass` | `builds` | `fixed` | `done` | `other`.
    pub claim_kind: Option<&'static str>,
    /// `Version`: which version.
    pub version: Option<HitVersion>,
}

/// How much of the project the corpus holds, by kind (after grouping).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Searched {
    pub requests: u64,
    pub statements: u64,
    pub files: u64,
    pub reads: u64,
    pub commands: u64,
    pub versions: u64,
    /// `Read` calls whose input names no file (not JSON, or no `file_path`):
    /// counted, never searched.
    pub reads_without_a_path: u64,
}

/// `GET /api/hq/projects/<id>/search` — the document.
#[derive(Debug, Clone, Serialize)]
pub struct RecordSearch {
    pub schema_version: u32,
    pub project_id: String,
    /// The thread the caller named as open, or `null`.
    pub thread: Option<String>,
    pub limit: usize,
    /// Everything that matched; `results` holds the first `limit` of it.
    pub total: u64,
    /// `total`, by kind; every kind is present.
    pub by_kind: BTreeMap<&'static str, u64>,
    pub results: Vec<Hit>,
    pub searched: Searched,
    pub masking: MaskRecord,
    /// Matching entries hidden from the searchable corpus by a path
    /// exclusion. Absent for a no-op so old views keep the same bytes.
    #[serde(skip_serializing_if = "is_zero")]
    pub left_out_by_exclusion: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Debug, Clone)]
enum Detail {
    Request {
        work: bool,
    },
    Path,
    Command {
        outcome: Outcome,
        exit_code: Option<i64>,
        tally: Option<HitTally>,
    },
    Said {
        verdict: Verdict,
        claim_kind: &'static str,
    },
    Version(HitVersion),
}

#[derive(Debug, Clone)]
struct Entry {
    kind: FindKind,
    thread: usize,
    /// Masked, whole.
    text: String,
    /// Lower-cased masked text that is searched (a version's also holds its
    /// commit and message).
    hay: String,
    /// The masking of every string this entry serves.
    record: MaskRecord,
    ms: i64,
    at: Option<String>,
    event_id: String,
    count: u64,
    source: String,
    detail: Detail,
}

/// One project's searchable record, built from one [`Index`].
#[derive(Debug, Clone)]
pub struct Corpus {
    project_id: String,
    threads: Vec<String>,
    titles: Vec<(String, MaskRecord)>,
    entries: Vec<Entry>,
    searched: Searched,
    left_out_by_exclusion: u64,
}

/// The typed text as the palette splits it: the whole of it, lower-cased and
/// trimmed, and its words.
pub fn terms(query: &str) -> (String, Vec<String>) {
    let whole = query.trim().to_lowercase();
    let cleaned: String = whole
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '/' | '.' | '_' | '-') {
                c
            } else {
                ' '
            }
        })
        .collect();
    let mut words: Vec<String> = Vec::new();
    for w in cleaned.split(' ') {
        if w.chars().count() >= 4 && !STOP_WORDS.contains(&w) && !words.iter().any(|x| x == w) {
            words.push(w.to_string());
        }
    }
    (whole, words)
}

fn matches(hay: &str, whole: &str, words: &[String]) -> bool {
    !whole.is_empty() && (hay.contains(whole) || words.iter().any(|w| hay.contains(w.as_str())))
}

fn excluded_text_key<'a>(
    text: &str,
    policy: &'a ExclusionPolicy,
    is_generated: &impl Fn(&str) -> bool,
) -> Option<&'a str> {
    policy.key_in_text_with_generated(text, is_generated)
}

/// A long text cut to at most `max` characters around its first match, never
/// through a `[masked: …]` marker, with `…` where it was cut.
fn window(text: &str, whole: &str, words: &[String], max: usize) -> String {
    // The index holds masked text; it is masked again here, before anything is
    // cut, so this cut never depends on its caller having done it.
    let (masked_text, _) = crate::redact::mask_recorded(text);
    let text = masked_text.as_str();
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    // The lower-cased text, and for each of its bytes the character of
    // `text` it came from.
    let mut low = String::new();
    let mut from: Vec<usize> = Vec::new();
    for (k, c) in chars.iter().enumerate() {
        for l in c.to_lowercase() {
            let before = low.len();
            low.push(l);
            from.extend(std::iter::repeat_n(k, low.len() - before));
        }
    }
    let hit = low
        .find(whole)
        .or_else(|| words.iter().filter_map(|w| low.find(w.as_str())).min())
        .and_then(|b| from.get(b).copied())
        .unwrap_or(0);
    let budget = max.saturating_sub(2).max(1);
    let mut start = hit.saturating_sub(budget / 3);
    if start + budget > chars.len() {
        start = chars.len().saturating_sub(budget);
    }
    // Never start inside a marker: move back to its opening bracket.
    let open: Vec<char> = "[masked: ".chars().collect();
    for k in 0..start {
        if chars[k..].starts_with(&open) {
            let end = chars[k..].iter().position(|c| *c == ']').map(|e| k + e);
            if end.is_none_or(|e| e >= start) {
                start = k;
                break;
            }
        }
    }
    let rest: String = chars[start..].iter().collect();
    let kept = truncate_respecting_markers(&rest, budget);
    let kept_len = kept.chars().count();
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(&kept);
    if start + kept_len < chars.len() {
        out.push('…');
    }
    out
}

#[derive(Default)]
struct Group {
    count: u64,
    /// The latest row, by the index's order key.
    latest: Option<usize>,
}

impl Group {
    fn add(&mut self, index: &Index, i: usize) {
        self.count += 1;
        if self
            .latest
            .is_none_or(|l| index.order_key(i) >= index.order_key(l))
        {
            self.latest = Some(i);
        }
    }
}

impl Corpus {
    /// The corpus of one project's index. Reads each `Read` call's input by
    /// primary key; writes nothing.
    pub fn build(ledger: &Ledger, index: &Index) -> Result<Corpus> {
        Corpus::build_on(ledger.conn(), index)
    }

    /// [`Corpus::build`] on any connection to the ledger the index was built
    /// from.
    pub fn build_on(conn: &Connection, index: &Index) -> Result<Corpus> {
        let rows = &index.rows;
        let roots: Vec<String> = std::iter::once(index.project.root.clone())
            .chain(index.project.members.iter().map(|m| m.root.clone()))
            .collect();
        let source = |i: usize| index.sessions[rows[i].session].source.clone();
        let mut entries: Vec<Entry> = Vec::new();
        let mut searched = Searched::default();
        let push = |entries: &mut Vec<Entry>,
                    kind: FindKind,
                    thread: usize,
                    raw: &str,
                    extra: &str,
                    i: usize,
                    event_id: String,
                    count: u64,
                    detail: Detail| {
            let mut record = MaskRecord::default();
            let (text, one) = crate::redact::mask_recorded(raw);
            record.merge(&one);
            let hay = if extra.is_empty() {
                text.to_lowercase()
            } else {
                format!("{text} {extra}").to_lowercase()
            };
            entries.push(Entry {
                kind,
                thread,
                text,
                hay,
                record,
                ms: rows[i].ms,
                at: rows[i].ts.clone(),
                event_id,
                count,
                source: source(i),
                detail,
            });
        };

        let mut titles = Vec::with_capacity(index.threads.len());
        let mut read_calls: Vec<(usize, usize)> = Vec::new();
        for (ti, t) in index.threads.iter().enumerate() {
            let mut record = MaskRecord::default();
            let (title, _, _) = index.title_of(t, &mut record);
            titles.push((title, record));

            // Requests: each person's prompt, as the index holds it.
            let mut prompts: Vec<(usize, bool)> = t
                .work_prompts
                .iter()
                .map(|&i| (i, true))
                .chain(t.conversation.iter().map(|&i| (i, false)))
                .collect();
            prompts.sort_by_key(|(i, _)| index.order_key(*i));
            for (i, work) in prompts {
                let Some(text) = rows[i].prompt.as_deref() else {
                    continue;
                };
                push(
                    &mut entries,
                    FindKind::Request,
                    ti,
                    text,
                    "",
                    i,
                    rows[i].id.clone(),
                    1,
                    Detail::Request { work },
                );
                searched.requests += 1;
            }

            // Edited files, reads, commands: grouped within the thread.
            let mut files: BTreeMap<&str, Group> = BTreeMap::new();
            let mut cmds: BTreeMap<(&str, bool), (Group, usize)> = BTreeMap::new();
            for &i in &t.rows {
                let r = &rows[i];
                for f in &r.files {
                    files.entry(f.as_str()).or_default().add(index, i);
                }
                for (k, c) in r.cmds.iter().enumerate() {
                    let Some(text) = c.text.as_deref() else {
                        continue;
                    };
                    let slot = cmds.entry((text, c.test)).or_insert((Group::default(), k));
                    let before = slot.0.latest;
                    slot.0.add(index, i);
                    if slot.0.latest != before {
                        slot.1 = k;
                    }
                }
                if r.kind == Kind::ToolUse
                    && r.tool_name
                        .as_deref()
                        .is_some_and(|n| READ_TOOLS.contains(&n))
                {
                    read_calls.push((ti, i));
                }
            }
            for (path, g) in files {
                let Some(i) = g.latest else { continue };
                push(
                    &mut entries,
                    FindKind::File,
                    ti,
                    path,
                    "",
                    i,
                    rows[i].id.clone(),
                    g.count,
                    Detail::Path,
                );
                searched.files += 1;
            }
            for ((text, test), (g, k)) in cmds {
                let Some(i) = g.latest else { continue };
                let c = &rows[i].cmds[k];
                let outcome = index.outcome(i, c);
                let output = rows[i]
                    .result
                    .and_then(|ri| index.result_text.get(&ri))
                    .map(String::as_str);
                let one_command = !(c.codemode && rows[i].cmds.len() != 1);
                let exit_code = if test && one_command {
                    output.and_then(commands::exit_code)
                } else {
                    None
                };
                let t_ = if test {
                    output.and_then(tally::tally).map(|t| HitTally {
                        passed: t.passed,
                        failed: t.failed,
                        skipped: t.skipped,
                        errors: t.errors,
                    })
                } else {
                    None
                };
                push(
                    &mut entries,
                    if test {
                        FindKind::Run
                    } else {
                        FindKind::Command
                    },
                    ti,
                    text,
                    "",
                    i,
                    rows[i].id.clone(),
                    g.count,
                    Detail::Command {
                        outcome,
                        exit_code,
                        tally: t_,
                    },
                );
                searched.commands += 1;
            }

            // Statements, with the statements route's verdicts.
            let judged: HashMap<(usize, usize), super::statements::Judged> =
                judge_thread(index, t).into_iter().collect();
            for &i in &t.rows {
                let Some(found) = index.claims.get(&i) else {
                    continue;
                };
                for (n, c) in found.iter().enumerate() {
                    let kind = claim_kind(c);
                    let verdict = judged.get(&(i, n)).map_or(
                        if matches!(kind, "tests_pass" | "builds") {
                            Verdict::NotProven
                        } else {
                            Verdict::Recorded
                        },
                        |j| j.verdict,
                    );
                    push(
                        &mut entries,
                        FindKind::Said,
                        ti,
                        &c.text,
                        "",
                        i,
                        format!("{}#{n}", rows[i].id),
                        1,
                        Detail::Said {
                            verdict,
                            claim_kind: kind,
                        },
                    );
                    searched.statements += 1;
                }
            }
        }

        // Reads: each call's path, by primary key, grouped by thread and path.
        let mut read_groups: BTreeMap<(usize, String), Group> = BTreeMap::new();
        let paths = read_paths(conn, index, &read_calls)?;
        for ((ti, i), path) in read_calls.iter().zip(paths) {
            match path {
                Some(p) => read_groups
                    .entry((*ti, display_path(&p, &roots)))
                    .or_default()
                    .add(index, *i),
                None => searched.reads_without_a_path += 1,
            }
        }
        for ((ti, path), g) in read_groups {
            let Some(i) = g.latest else { continue };
            push(
                &mut entries,
                FindKind::Read,
                ti,
                &path,
                "",
                i,
                rows[i].id.clone(),
                g.count,
                Detail::Path,
            );
            searched.reads += 1;
        }

        // Versions: each once, in the thread that holds its commit, else the
        // thread that holds its latest row.
        let mut holder: HashMap<usize, (usize, usize)> = HashMap::new();
        for (ti, t) in index.threads.iter().enumerate() {
            for &i in &t.rows {
                let v = rows[i].version;
                let Some(ver) = index.versions.get(v) else {
                    continue;
                };
                let boundary = ver.boundary == Some(i);
                let take = match holder.get(&v) {
                    None => true,
                    Some(&(_, held)) => {
                        let held_boundary = ver.boundary == Some(held);
                        !held_boundary && (boundary || index.order_key(i) > index.order_key(held))
                    }
                };
                if take {
                    holder.insert(v, (ti, i));
                }
            }
        }
        let mut held: Vec<(usize, (usize, usize))> = holder.into_iter().collect();
        held.sort_unstable();
        for (v, (ti, i)) in held {
            let ver = &index.versions[v];
            let current = index.threads[ti]
                .rows
                .last()
                .is_some_and(|&last| rows[last].version == v);
            let mut record = MaskRecord::default();
            let message = ver.message.as_deref().map(|m| masked(m, &mut record));
            let short = ver.commit.as_ref().map(|c| c.chars().take(7).collect());
            let shown: String = short.clone().unwrap_or_else(|| ver.id.clone());
            let extra = format!(
                "{} {} {}",
                ver.id,
                ver.commit.as_deref().unwrap_or(""),
                message.as_deref().unwrap_or("")
            );
            let before = entries.len();
            push(
                &mut entries,
                FindKind::Version,
                ti,
                &shown,
                &extra,
                i,
                rows[i].id.clone(),
                1,
                Detail::Version(HitVersion {
                    id: ver.id.clone(),
                    short,
                    commit: ver.commit.clone(),
                    committed: ver.committed,
                    message,
                    current,
                }),
            );
            if let Some(e) = entries.get_mut(before) {
                e.record.merge(&record);
            }
            searched.versions += 1;
        }

        Ok(Corpus {
            project_id: index.project.id.clone(),
            threads: index.threads.iter().map(|t| t.id.clone()).collect(),
            titles,
            entries,
            searched,
            left_out_by_exclusion: 0,
        })
    }

    /// Make the corpus searchable only through what the person is allowed to
    /// see now. An excluded hit remains as its fixed marker, so asking for the
    /// original path cannot find it, while a search for the marker can explain
    /// why a row is missing. The predicate says whether a path is marked
    /// generated by the repository; ordinary policies need no such lookup.
    pub fn apply_exclusions(
        &mut self,
        policy: &ExclusionPolicy,
        is_generated: impl Fn(&str) -> bool,
    ) -> u64 {
        if policy.is_empty() {
            return 0;
        }
        let mut left_out = 0;
        for (title, _) in &mut self.titles {
            if let Some(key) = excluded_text_key(title, policy, &is_generated) {
                *title = exclusion_marker(key);
                left_out += 1;
            }
        }
        for entry in &mut self.entries {
            let key = match entry.kind {
                FindKind::File | FindKind::Read => {
                    policy.key_for_path(&entry.text, is_generated(&entry.text))
                }
                FindKind::Run | FindKind::Command => {
                    policy.key_in_command_with_generated(&entry.text, &is_generated)
                }
                FindKind::Version => match &entry.detail {
                    Detail::Version(v) => v
                        .message
                        .as_deref()
                        .and_then(|m| excluded_text_key(m, policy, &is_generated)),
                    _ => None,
                },
                FindKind::Request | FindKind::Said => {
                    excluded_text_key(&entry.text, policy, &is_generated)
                }
            };
            let key = key.or_else(|| {
                matches!(entry.kind, FindKind::Run | FindKind::Command)
                    .then(|| excluded_text_key(&entry.text, policy, &is_generated))
                    .flatten()
            });
            if let Some(key) = key {
                let mark = exclusion_marker(key);
                entry.text = mark.clone();
                // The marker includes the key for disclosure, but matching
                // that key would rediscover the very row it left out.
                entry.hay = "left out by your exclusion".into();
                if let Detail::Version(v) = &mut entry.detail {
                    v.message = Some(mark);
                }
                left_out += 1;
            }
        }
        self.left_out_by_exclusion = left_out;
        left_out
    }

    /// Original strings whose file names can be marked generated. The caller
    /// resolves Git attributes once, then passes the resulting membership to
    /// [`Corpus::apply_exclusions`].
    pub fn searchable_texts(&self) -> Vec<&str> {
        let mut texts = Vec::with_capacity(self.entries.len() + self.titles.len());
        texts.extend(self.titles.iter().map(|(title, _)| title.as_str()));
        for entry in &self.entries {
            texts.push(entry.text.as_str());
            if let Detail::Version(version) = &entry.detail {
                if let Some(message) = version.message.as_deref() {
                    texts.push(message);
                }
            }
        }
        texts
    }

    /// How many things the corpus holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the project holds nothing to search.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The bytes of text the corpus keeps (served and searched copies), for
    /// stating its memory cost.
    pub fn text_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|e| e.text.len() + e.hay.len() + e.event_id.len())
            .sum::<usize>()
            + self.titles.iter().map(|(t, _)| t.len()).sum::<usize>()
    }

    /// True when the project holds this thread.
    pub fn has_thread(&self, thread_id: &str) -> bool {
        self.threads.iter().any(|t| t == thread_id)
    }

    /// Every thing that matches `query`, newest first, the open thread's
    /// first; the first `limit` (at most [`MAX_LIMIT`]) are served. Of the
    /// thread named `open`, only `Read` and `Version` things are served
    /// ([`FindKind::in_thread_view`]).
    pub fn search(&self, query: &str, limit: usize, open: Option<&str>) -> RecordSearch {
        let (whole, words) = terms(query);
        let open_ix = open.and_then(|tid| self.threads.iter().position(|t| t == tid));
        let mut found: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| !(Some(e.thread) == open_ix && e.kind.in_thread_view()))
            .filter(|e| matches(&e.hay, &whole, &words))
            .collect();
        let mut by_kind: BTreeMap<&'static str, u64> =
            FindKind::ALL.iter().map(|k| (k.name(), 0)).collect();
        for e in &found {
            *by_kind.entry(e.kind.name()).or_default() += 1;
        }
        found.sort_by(|a, b| {
            (Some(b.thread) == open_ix)
                .cmp(&(Some(a.thread) == open_ix))
                .then(b.ms.cmp(&a.ms))
                .then(a.kind.cmp(&b.kind))
                .then(a.text.cmp(&b.text))
                .then(a.event_id.cmp(&b.event_id))
        });
        let limit = limit.clamp(1, MAX_LIMIT);
        let mut masking = MaskRecord::default();
        let mut results = Vec::new();
        for e in found.iter().take(limit) {
            masking.merge(&e.record);
            let (title, title_record) = &self.titles[e.thread];
            masking.merge(title_record);
            let mut hit = Hit {
                kind: e.kind,
                thread_id: self.threads[e.thread].clone(),
                thread_title: title.clone(),
                open_thread: Some(e.thread) == open_ix,
                text: window(&e.text, &whole, &words, TEXT_WINDOW_CHARS),
                at: e.at.clone(),
                event_id: e.event_id.clone(),
                count: e.count,
                source: e.source.clone(),
                work: None,
                outcome: None,
                exit_code: None,
                tally: None,
                verdict: None,
                claim_kind: None,
                version: None,
            };
            match &e.detail {
                Detail::Request { work } => hit.work = Some(*work),
                Detail::Path => {}
                Detail::Command {
                    outcome,
                    exit_code,
                    tally,
                } => {
                    hit.outcome = Some(*outcome);
                    hit.exit_code = *exit_code;
                    hit.tally = *tally;
                }
                Detail::Said {
                    verdict,
                    claim_kind,
                } => {
                    hit.verdict = Some(*verdict);
                    hit.claim_kind = Some(claim_kind);
                }
                Detail::Version(v) => hit.version = Some(v.clone()),
            }
            results.push(hit);
        }
        RecordSearch {
            schema_version: SCHEMA_VERSION,
            project_id: self.project_id.clone(),
            thread: open_ix.map(|i| self.threads[i].clone()),
            limit,
            total: found.len() as u64,
            by_kind,
            results,
            searched: self.searched.clone(),
            masking,
            left_out_by_exclusion: self.left_out_by_exclusion,
        }
    }
}

/// The file each `Read` call names, in `calls` order: `None` when its input
/// is not JSON or names no `file_path`. By primary key, 400 at a time.
fn read_paths(
    conn: &Connection,
    index: &Index,
    calls: &[(usize, usize)],
) -> Result<Vec<Option<String>>> {
    let mut by_id: HashMap<String, Option<String>> = HashMap::with_capacity(calls.len());
    for chunk in calls.chunks(400) {
        let marks = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT id, CASE WHEN json_valid(tool_input)
                             THEN json_extract(tool_input, '$.file_path') END
               FROM events WHERE id IN ({marks})"
        ))?;
        let found = stmt.query_map(
            params_from_iter(chunk.iter().map(|(_, i)| index.rows[*i].id.as_str())),
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<rusqlite::types::Value>>(1)?,
                ))
            },
        )?;
        for row in found {
            let (id, value) = row?;
            let path = match value {
                Some(rusqlite::types::Value::Text(p)) if !p.trim().is_empty() => Some(p),
                _ => None,
            };
            by_id.insert(id, path);
        }
    }
    Ok(calls
        .iter()
        .map(|(_, i)| by_id.get(&index.rows[*i].id).cloned().flatten())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_typed_text_splits_as_the_palette_splits_it() {
        let (whole, words) = terms("  Was a MIGRATION run?  ");
        assert_eq!(whole, "was a migration run?");
        assert_eq!(words, vec!["migration".to_string()]);
        let (_, words) = terms("what changed after the last check");
        assert_eq!(
            words,
            vec![
                "changed".to_string(),
                "last".to_string(),
                "check".to_string()
            ]
        );
        let (_, words) = terms("src/a.rs totals.py");
        assert_eq!(words, vec!["src/a.rs".to_string(), "totals.py".to_string()]);
    }

    #[test]
    fn a_match_is_the_whole_text_or_one_word() {
        let (whole, words) = terms("was a migration run?");
        assert!(matches("ran the migration", &whole, &words));
        assert!(!matches("ran the tests", &whole, &words));
        // a word under four characters matches only as part of the whole text
        let (whole, words) = terms("rm");
        assert!(words.is_empty());
        assert!(matches("rm -rf build", &whole, &words));
        assert!(!matches("", &whole, &words));
    }

    #[test]
    fn a_long_text_is_cut_around_the_match_and_never_through_a_marker() {
        let long = format!("{} the migration ran {}", "a".repeat(300), "b".repeat(300));
        let (whole, words) = terms("migration");
        let w = window(&long, &whole, &words, 160);
        assert!(w.contains("migration"), "{w}");
        assert!(w.starts_with('…') && w.ends_with('…'), "{w}");
        assert!(w.chars().count() <= 160, "{}", w.chars().count());

        // the match sits just after a marker the window would otherwise start in
        let marked = format!(
            "{} [masked: aws_access_key] {} migration {}",
            "x".repeat(200),
            "z".repeat(40),
            "y".repeat(200)
        );
        let w = window(&marked, &whole, &words, 160);
        assert!(w.contains("migration"), "{w}");
        if w.contains("aws_access_key") {
            assert!(
                w.contains("[masked: aws_access_key]"),
                "a marker whole: {w}"
            );
        }
        assert_eq!(w.matches('[').count(), w.matches(']').count(), "{w}");

        assert_eq!(window("short", &whole, &words, 160), "short");
    }
}
