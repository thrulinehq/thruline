//! HQ: every prompt, every edit, every command and every claim a project's
//! recorded sessions hold, connected into threads of work.
//!
//! Read-only and deterministic. Nothing here writes to the ledger, calls a
//! model or touches the network; the same rows and the same `now` give the same
//! answer. Every served string has been through [`crate::redact`] before it
//! leaves this module (see [`masked`]).
//!
//! # What a project is
//!
//! The ledger files a recording under a *ledger project*: one per Claude Code
//! folder (`~/.claude/projects/<mangled root>`) and one per Codex working
//! directory. An **HQ project** is the set of ledger projects that recorded
//! work in one repository root:
//!
//! 1. Each ledger project's root is the `cwd` its newest recordings state
//!    (Claude Code; read off the transcript's first lines, newest file first),
//!    else the root the ledger stored (Codex stores the real path).
//! 2. A root under `<root>/.claude/worktrees/…` belongs to `<root>`.
//! 3. A root inside another ledger project's root belongs to that outer root
//!    when the outer root is a git repository on disk (it has `.git`). The
//!    outermost such root wins. (A home directory with stray recordings is
//!    not a repository, so it does not swallow every project under it.)
//!
//! 4. A ledger project whose root (rule 1) is scratch — inside
//!    a temporary folder (`/tmp`, `/private/tmp`, `/var/folders`, the OS temp
//!    dir) and not inside a git repository on disk — is not a project at all;
//!    it is counted as `excluded_temp_projects`.
//!
//! The HQ project id is the first 16 hex characters of
//! `sha256("hq-project-v1\0" + root)` — stable across reads and stores.
//!
//! # What a person's prompt is
//!
//! A `user_msg` that is not sidechain, not in a Codex sub-agent session (whose
//! "user" is the parent agent — read off the session's own `session_meta`), and
//! that [`prompts::classify`] calls `Human`: not one of the injected envelopes
//! in [`prompts::INJECTED_PREFIXES`], not markup only, not a bare slash-command
//! wrapper (a slash command WITH arguments is the person's prompt,
//! `/name args`), not the expanded command body that follows a wrapper
//! within [`WRAPPER_BODY_MS`] (2 s).
//!
//! A **turn** is a person's prompt and every row after it in the same session
//! up to the next person's prompt. A **work prompt** is one whose turn holds at
//! least one edit (`Edit`, `Write`, `MultiEdit`, `NotebookEdit`, a Codex patch)
//! or command (`Bash`, a Codex `exec_command`) — see [`codemode`] for how
//! Codex code-mode programs are read.
//!
//! # Threads (the "connect them" rule)
//!
//! Sessions are taken in order of their first recorded instant.
//!
//! 1. **Copied history.** A resumed or forked transcript starts with a
//!    verbatim copy of its parent's rows (same `src_uuid`). Copies are not
//!    counted twice: a row whose `src_uuid` was already seen in an earlier
//!    session is skipped, and the thread the original belonged to stays open
//!    into the new session.
//! 2. **Continuation preamble.** A session whose first `user_msg` is Claude
//!    Code's "This session is being continued…" preamble, and that shares no
//!    history, continues the thread that was open at the end of the session
//!    whose last row is the latest at or before this session's first row.
//! 3. **Within a session**, a work prompt joins the open thread unless the
//!    files it edits are *disjoint* from the thread's files AND either the
//!    thread has had a commit, or more than [`SPLIT_GAP_MS`] (6 h) passed since
//!    the thread's last row. A turn that edits no file is never disjoint (it
//!    cannot be judged), so it stays. Otherwise the work prompt opens a new
//!    thread. A non-work prompt joins the open thread as conversation, or —
//!    with no thread open — is counted as `conversation_only`.
//! 4. **Codex sub-agent sessions** open no thread: their rows join the parent
//!    session's thread that was open when the sub-agent started, or are
//!    counted as `unattached_turns`.
//! 5. **Across sessions**, two threads that share no session merge when both
//!    edited files, their activity windows are within [`MERGE_WINDOW_MS`]
//!    (72 h) of each other, and the Jaccard index of their edited-file sets is
//!    at least [`MERGE_JACCARD`] (0.3). Pairs are judged on the threads as
//!    built by rules 1–4, and merging is transitive.
//!
//! A thread's id is the first 16 hex characters of `sha256(event_id)` of its
//! first work prompt (the earliest, after merging).
//!
//! # Titles (C26)
//!
//! A title is a sentence the person wrote, verbatim, masked, then cut to 90
//! characters with `…` — chosen by [`prompts::thread_title`]:
//!
//! 1. In the first work prompt, the first sentence the person TYPED (outside
//!    pasted blocks) that is not skippable and names work (`work_sentence`).
//!    A sentence names work when, after fillers ("please", "can you", "I want
//!    you to"), it opens with an instruction verb that is not a go-on verb
//!    (continue, start, resume, proceed, go, carry, let, help, try) and an
//!    object follows (a word that is not a pronoun, particle or adverb, or a
//!    path / `code` token). Skippable: three words or fewer naming no work
//!    ("Start now."); a question naming no work that asks to go on ("Can you
//!    take over from here?"); a role or instruction preamble ("You are …",
//!    "Read … first", "Rulings:", "Context:", any line ending in `:`, a lane
//!    or role heading naming no work).
//! 2. Else a work sentence among the first six sentences of a pasted block
//!    (`pasted_work_sentence`) — never deeper in a pasted brief.
//! 3. Else the first later prompt of the thread with a work sentence
//!    (`work_sentence_later_prompt`). Decided this way because a thread that
//!    opens "Start now." and then asks for a change is about that change.
//! 4. Else the pre-C26 rule on the first prompt: the first instruction
//!    sentence (`fallback_instruction`), else the first sentence
//!    (`fallback_first_sentence`), else a pasted heading or first sentence.
//!    When that prompt holds no sentence at all (an image only), the first
//!    later prompt with any title. `title_event_id` names the prompt used.
//!
//! # Whose move
//!
//! * `agent` — the thread's last row is a prompt, tool call, tool result or
//!   assistant row stamped within [`AGENT_ACTIVE_MS`] (10 min) of `now`.
//! * `you` — otherwise, when any of: an open claim of finished work
//!   ([`crate::extract::MarkerExtractor`], kinds `work`/`marker`) — open
//!   unless the latest test run before it passed with no edit in between, or
//!   a passing test run follows it; the last test run failed; the agent's last words end in a question, or its last
//!   tool call asked the person (`AskUserQuestion`,
//!   `request_user_input_async`).
//! * `done` — otherwise.
//!
//! `proof` carries the last test run and `edited_after_last_pass`: the files
//! edited (successfully or not) after the last passing test run.
//!
//! # Versions
//!
//! A session's rows are cut into versions at each `git commit` that took
//! effect (the result shows a commit hash, or the command succeeded). A
//! version holds every row up to and including the commit that closes it; its
//! id is the 7-character hash the result shows, else
//! `wt-<16 hex of sha256(session_id:from_seq)>`. The rows after the last
//! commit are an uncommitted version (`committed: false`). Every edit,
//! command and claim carries the `version_id` current at its row.

pub mod codemode;
pub mod commands;
pub mod edits;
pub mod load;
pub mod prompts;
pub mod search;
pub mod statements;
pub mod tally;
mod view;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{params, params_from_iter, Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;

use crate::chain::sha256_hex;
use crate::error::Result;
use crate::extract::{ClaimExtractor, ClaimKind, MarkerExtractor};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

pub use load::{
    apply_pause_gate, default_workers, load_order, run_load, run_load_with, LoadFileTiming,
    LoadOutcome,
};
pub use view::{
    session, thread, ClaimItem, CommandItem, EditItem, FileTouch, PromptItem, SessionItem,
    SessionTimeline, ThreadView, TimelineItem, VersionItem,
};

/// Bump when a served field changes meaning or disappears.
pub const SCHEMA_VERSION: u32 = 1;
/// Rule 3: the gap after which disjoint work opens a new thread.
pub const SPLIT_GAP_MS: i64 = 6 * 3600 * 1000;
/// Rule 5: the largest gap between two threads' windows that may merge.
pub const MERGE_WINDOW_MS: i64 = 72 * 3600 * 1000;
/// Rule 5: the least Jaccard index of edited files that merges.
pub const MERGE_JACCARD: f64 = 0.3;
/// A user row this close to a slash-command wrapper is the wrapper's
/// expanded body, not a person's prompt.
pub const WRAPPER_BODY_MS: i64 = 2_000;
/// Whose move: how recent the last row must be for the agent to be working.
pub const AGENT_ACTIVE_MS: i64 = 10 * 60 * 1000;

/* ───────────────────────────── served shapes ───────────────────────────── */

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Counts {
    pub total: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub unknown: u64,
}

impl Counts {
    fn add(&mut self, outcome: Outcome) {
        self.total += 1;
        match outcome {
            Outcome::Succeeded => self.succeeded += 1,
            Outcome::Failed => self.failed += 1,
            Outcome::Unknown => self.unknown += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WhoseMove {
    You,
    Agent,
    Done,
}

#[derive(Debug, Clone, Serialize)]
pub struct Member {
    /// The ledger project id.
    pub ledger_project_id: String,
    pub source: String,
    /// The root this member's recordings name — or, when `root_basis` is
    /// `folder_name`, the vendor's folder name as stored, which is **not** a
    /// path (nothing has stated where the folder is yet).
    pub root: String,
    /// `recorded_cwd` | `known_root` (a worktree folder, or the exact folder, of
    /// a root some recording states) | `stored_root` (Codex: the real path; or a
    /// Claude Code folder registered before any recording, whose stored root is
    /// not what its name decodes to) | `folder_name` (Claude Code, nothing states
    /// the path: `root` is the folder name as stored, see [`path_known`](HqProject::path_known)).
    pub root_basis: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    /// Always `true` here: every count below was derived from the index. A
    /// list row served before the index exists is a [`QuickSummary`], with
    /// `complete: false`.
    pub complete: bool,
    pub id: String,
    pub name: String,
    pub root: String,
    /// `false` when nothing has stated where this folder is: `name` and `root`
    /// are then the vendor's folder name as stored, not a path, and the screen
    /// says "path not known until the first recording".
    pub path_known: bool,
    pub sources: Vec<String>,
    pub members: Vec<Member>,
    pub sessions: u64,
    /// Work prompts (see module docs).
    pub prompts: u64,
    /// A person's prompts that started no work.
    pub conversation_prompts: u64,
    /// File edits: one per file per edit call.
    pub edits: u64,
    pub commands: Counts,
    pub tests: Counts,
    /// Claims of finished work (`work`/`marker` kinds).
    pub claims: u64,
    pub first_activity: Option<String>,
    pub last_activity: Option<String>,
    pub threads: u64,
    pub conversation_only: u64,
    pub unattached_turns: u64,
    pub copied_rows_skipped: u64,
    pub whose_move: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestRef {
    pub event_id: String,
    /// `<event_id>` or `<event_id>#<n>` for a command inside a Codex program.
    pub command_id: String,
    pub at: Option<String>,
    pub outcome: Outcome,
    /// First line of the command, masked, ≤ 200 characters.
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WaitingCommand {
    /// The unanswered command's first line, masked like the other command summaries.
    pub command: String,
    pub step: u64,
    pub at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestClaim {
    pub agent: String,
    /// A masked sentence from the agent's recorded claim.
    pub text: String,
    pub at: Option<String>,
    pub step: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileRemoval {
    /// The command as recorded, masked; this is an observation, not a verdict.
    pub command: String,
    pub at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadTraceEvent {
    pub kind: &'static str,
    pub step: u64,
    pub file: Option<String>,
    pub result: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadVersionTrace {
    pub version: String,
    pub now: bool,
    pub ran: bool,
    pub events: Vec<ThreadTraceEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Proof {
    pub last_test: Option<TestRef>,
    pub last_passing_test_at: Option<String>,
    pub edited_after_last_pass: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileCount {
    pub path: String,
    pub edits: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub title_basis: prompts::TitleBasis,
    pub title_event_id: String,
    pub started_at: Option<String>,
    pub last_activity_at: Option<String>,
    pub sessions: u64,
    pub prompts: u64,
    pub conversation: u64,
    pub edits: u64,
    pub files: u64,
    pub top_files: Vec<FileCount>,
    pub commands: Counts,
    pub tests: Counts,
    pub commits: u64,
    pub claims: u64,
    pub whose_move: WhoseMove,
    /// Which rule decided `whose_move`: `recent_activity` |
    /// `claim_without_passing_test` | `last_test_failed` |
    /// `agent_asked` | `nothing_open`.
    pub whose_move_basis: &'static str,
    /// The actual unanswered question row when `agent_asked`. Absent if a
    /// later user row answered it, even when the older move basis still says
    /// `agent_asked`. The ID is an opaque record identity, never question text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_event_id: Option<String>,
    /// An unfinished command after the question, when the agent is waiting for the person.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_command: Option<WaitingCommand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_claim: Option<LatestClaim>,
    /// The newest recorded versions and steps for the Inbox's small thread drawing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub version_thread: Vec<ThreadVersionTrace>,
    /// Commands that removed files and have a recorded result; never an approval signal.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_removals: Vec<FileRemoval>,
    pub proof: Proof,
    pub current_version_id: Option<String>,
    /// C30 R-LEDGER (told-the-agent): the last time a person copied a message
    /// for the agent on this thread, or said they pasted it, and whether any
    /// work has been recorded on the thread since. Absent when the thread has
    /// no such act, so a project that has none serializes exactly as before.
    /// It is a mark in the person's own record: it moves no `whose_move`, no
    /// verdict and no count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub told_agent: Option<crate::change_agent_message::ToldAgent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectView {
    pub schema_version: u32,
    pub project: ProjectSummary,
    pub threads: Vec<ThreadSummary>,
    pub masking: MaskRecord,
}

/* ───────────────────────────── membership ───────────────────────────── */

/// One HQ project and the ledger projects it covers.
#[derive(Debug, Clone)]
pub struct HqProject {
    pub id: String,
    /// What a person reads: the last component of `root`, or — while the path
    /// is not known ([`path_known`](Self::path_known)) — the vendor's folder
    /// name as stored.
    pub name: String,
    /// The root this project is identified by and grouped under (`id` is
    /// hashed from it). While the path is not known it is the folder name
    /// decoded, which is a guess: it is never served or shown as a path (use
    /// [`shown_root`](Self::shown_root)), and nothing is installed or written
    /// there.
    pub root: String,
    pub members: Vec<Member>,
}

impl HqProject {
    /// Whether `root` is a path some record states. `false` for a Claude Code
    /// folder nothing has recorded in: `root` is then only the folder name
    /// decoded, and the screen says the path is not known until the first
    /// recording. (CTO 2026-10-12: ThruLine must not show a guessed path. The
    /// `id` is unchanged by this: it is still hashed from `root`.)
    pub fn path_known(&self) -> bool {
        path_known_of(&self.members)
    }

    /// The root as a person may be shown it: the stated path, or — while no
    /// record states one — the vendor's folder name as stored. Every served or
    /// printed root comes through here.
    pub fn shown_root(&self) -> String {
        if self.path_known() {
            return self.root.clone();
        }
        self.members
            .first()
            .map(|m| m.root.clone())
            .unwrap_or_else(|| self.root.clone())
    }
}

fn path_known_of(members: &[Member]) -> bool {
    members.iter().any(|m| m.root_basis != "folder_name")
}

/// The HQ id for a root.
pub fn project_id_for_root(root: &str) -> String {
    sha256_hex(format!("hq-project-v1\0{root}").as_bytes())[..16].to_string()
}

/// Every HQ project in the ledger, ordered by root, without the ones
/// recorded in temporary folders (see [`hq_projects_and_excluded`]).
pub fn hq_projects(ledger: &Ledger) -> Result<Vec<HqProject>> {
    Ok(hq_projects_and_excluded(ledger)?.0)
}

/// [`hq_projects`] on any connection to a ledger (see [`hq_projects_and_excluded_on`]).
pub fn hq_projects_on(conn: &Connection) -> Result<Vec<HqProject>> {
    Ok(hq_projects_and_excluded_on(conn)?.0)
}

/// Folders that hold scratch work, not repositories: `/tmp`, `/private/tmp`,
/// `/var/folders`, `/private/var/folders`, and the OS temporary directory.
pub fn temp_roots() -> Vec<String> {
    let mut roots: Vec<String> = [
        "/tmp",
        "/private/tmp",
        "/var/folders",
        "/private/var/folders",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let tmp = std::env::temp_dir();
    roots.push(tmp.display().to_string().trim_end_matches('/').to_string());
    if let Ok(real) = tmp.canonicalize() {
        roots.push(real.display().to_string().trim_end_matches('/').to_string());
    }
    roots.sort();
    roots.dedup();
    roots
}

/// Is this root inside a temporary folder ([`temp_roots`])?
pub fn is_temp_root(root: &str) -> bool {
    temp_roots().iter().any(|t| is_under(root, t))
}

/// Is this root scratch work: inside a temporary folder AND not inside a git
/// repository that is on disk now (no `.git` at the root or any of its
/// ancestors below the temporary folder)?
///
/// The carve-out is deliberate. An agent's scratch directory
/// (`/private/tmp/claude-501/…/scratchpad`) is not a repository and is often
/// gone already; a repository someone cloned into `/tmp` is real work, and so
/// is every repository this product's own test harness builds under the OS
/// temporary folder.
pub fn is_scratch_root(root: &str) -> bool {
    let Some(temp) = temp_roots().into_iter().find(|t| is_under(root, t)) else {
        return false;
    };
    let mut at = Some(Path::new(root));
    while let Some(dir) = at {
        if dir.as_os_str().len() <= temp.len() {
            break;
        }
        if dir.join(".git").exists() {
            return false;
        }
        at = dir.parent();
    }
    true
}

/// Every HQ project, and how many were left out because their recordings
/// were made in a temporary folder.
///
/// A ledger project is left out when its root — the recorded `cwd`, or the
/// stored root when no `cwd` could be read — is scratch ([`is_scratch_root`]: in a temporary folder and not in a git
/// repository on disk): Claude Code sessions run
/// inside scratch directories (`/private/tmp/claude-501/…/scratchpad`) are
/// filed like repositories, and listing them as projects was C21-d finding 3.
/// The count is of distinct roots left out.
pub fn hq_projects_and_excluded(ledger: &Ledger) -> Result<(Vec<HqProject>, u64)> {
    hq_projects_and_excluded_on(ledger.conn())
}

/// [`hq_projects_and_excluded`] on any connection to a ledger. Reads only, so
/// a caller that must not create or harden anything (a read-only HQ lookup,
/// HQ-NEEDS C28) may pass a connection it opened with `SQLITE_OPEN_READ_ONLY`.
pub fn hq_projects_and_excluded_on(conn: &Connection) -> Result<(Vec<HqProject>, u64)> {
    let mut stmt = conn.prepare("SELECT id, root_path, source FROM projects ORDER BY id")?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut excluded: BTreeSet<String> = BTreeSet::new();
    // First every root a recording states (or Codex stores), then the
    // folders with none decode against those ([`decode_folder_name`]).
    let mut stated: Vec<(String, String, String, Option<String>, bool)> = Vec::new();
    for (id, stored, source) in rows {
        let (cwd, unrecorded) = if source == "claude_code" {
            let files = recent_native_files_on(conn, &id, 5)?;
            (
                files.iter().find_map(|f| recorded_cwd(Path::new(f))),
                files.is_empty(),
            )
        } else {
            (Some(stored.clone()), false)
        };
        stated.push((id, stored, source, cwd, unrecorded));
    }
    let known: Vec<String> = stated.iter().filter_map(|s| s.3.clone()).collect();
    // One entry per ledger project: what is shown (`member`), the root its HQ
    // id and grouping have always been derived from (`identity`: the decoded
    // folder name when nothing states a path, so ids do not move), and whether
    // `member.root` is a path at all.
    struct Placed {
        member: Member,
        identity: String,
        path_known: bool,
    }
    let mut placed: Vec<Placed> = Vec::new();
    for (id, stored, source, cwd, unrecorded) in stated {
        let (root, basis, path_known) = match cwd {
            Some(cwd) if source == "claude_code" => (cwd, "recorded_cwd", true),
            Some(cwd) => (cwd, "stored_root", true),
            // A Claude Code project id IS the vendor's folder name; its
            // stored root is that name with every `-` made `/`, which loses
            // dots (`.claude` → `/claude`) and every real hyphen. Decode it
            // against what the record states, and say when that is a guess.
            None => {
                let (root, basis, certain) = decode_folder(&id, &known);
                if certain {
                    (root, basis, true)
                } else if stored_root_was_picked(&id, &stored, unrecorded) {
                    // A folder a person picked before anything was recorded:
                    // the registration stored the path they picked, which is
                    // not a reading of the folder name.
                    (stored, "stored_root", true)
                } else {
                    (root, "folder_name", false)
                }
            }
        };
        // The recorded `cwd` when there is one; the stored root only when not
        // (a Claude Code stored root is the folder name un-mangled, which is
        // lossy: `.tmpX` comes back as `/tmpX`). A guess is still good enough
        // to tell a scratch folder from a repository; it is never shown.
        if is_scratch_root(&root) {
            excluded.insert(worktree_parent(&root));
            continue;
        }
        let identity = worktree_parent(&root);
        placed.push(Placed {
            member: Member {
                root: if path_known {
                    identity.clone()
                } else {
                    // The folder name as stored, not a path.
                    id.clone()
                },
                ledger_project_id: id,
                source,
                root_basis: basis,
            },
            identity,
            path_known,
        });
    }
    // Only a stated path can sit inside another: a folder name says nothing
    // about which directory it is, so it neither folds into a repository nor
    // takes another under it.
    let roots: BTreeSet<String> = placed
        .iter()
        .filter(|p| p.path_known)
        .map(|p| p.identity.clone())
        .collect();
    // grouping key → (the root shown for the group, its members)
    let mut by_root: BTreeMap<String, (Option<String>, Vec<Member>)> = BTreeMap::new();
    for p in placed {
        let (key, shown) = if p.path_known {
            // Outermost git-repository root that contains this one.
            let canonical = roots
                .iter()
                .filter(|r| {
                    is_under(&p.identity, r)
                        && *r != &p.identity
                        && Path::new(r).join(".git").exists()
                })
                .min_by_key(|r| r.len())
                .cloned()
                .unwrap_or_else(|| p.identity.clone());
            (canonical.clone(), Some(canonical))
        } else {
            (p.identity, None)
        };
        let entry = by_root.entry(key).or_insert((None, Vec::new()));
        if entry.0.is_none() {
            entry.0 = shown.or_else(|| Some(p.member.root.clone()));
        } else if shown.is_some() && !path_known_of(&entry.1) {
            // A stated path joins a group a folder name opened: the path wins.
            entry.0 = shown;
        }
        entry.1.push(p.member);
    }
    let projects = by_root
        .into_iter()
        .map(|(key, (shown, members))| {
            // What a person reads as the name: the last component of the
            // stated path, or the folder name as stored.
            let shown = shown.unwrap_or_else(|| key.clone());
            HqProject {
                id: project_id_for_root(&key),
                name: Path::new(&shown)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| shown.clone()),
                root: key,
                members,
            }
        })
        .collect();
    Ok((projects, excluded.len() as u64))
}

/// The repository path a Claude Code project folder name stands for, and how
/// it was found — for when no recording states a `cwd`.
///
/// The vendor turns every character that is not a letter or digit into `-`
/// ([`crate::speaker::mangle_root`]), so the name alone cannot say whether a
/// `-` was `/`, `.`, `_`, ` ` or `-`. Two ways, in order, and neither touches
/// the filesystem (see below):
///
/// 1. **`known_root`** — a root some recording states (`known`) whose mangled
///    form is the folder name, or a leading part of it followed by `-`: that
///    real path, plus the rest of the name decoded as in 2. The longest such
///    root wins. This is how a worktree folder with no `cwd` of its own still
///    lands inside its repository (`…/Telescope_project/.claude/worktrees/x`).
/// 2. **`folder_name`** — `--` decodes as `/.` (a hidden directory such as
///    `.claude`) and every other `-` as `/`. Lossy, and said so by the basis.
///
/// **Why no filesystem check.** Walking the disk to match names was tried and
/// measured on 2026-10-07: listing a folder under `~/Desktop` or `~/Documents`
/// blocks inside `opendir` until macOS privacy consent is given (it sat there
/// for ten minutes), and an external volume that is not mounted can do the
/// same. A server request must never wait on either, so names are decoded
/// against what the record already says.
pub fn decode_folder_name(name: &str, known: &[String]) -> (String, &'static str) {
    let (path, basis, _) = decode_folder(name, known);
    (path, basis)
}

/// [`decode_folder_name`], and whether the path it found is one the record
/// states rather than one the name suggests. Certain: a folder whose mangled
/// name IS a stated root, or is a worktree (`--claude-worktrees-`) directly
/// inside one — `worktree_parent` then folds it into that root, and nothing
/// of the guessed tail is left. Not certain: every other decoding, because a
/// `-` in the name may be `/`, `.`, `_`, a space or a real `-`, and a sibling
/// such as `proj-two` begins with `proj-` as `proj/two` does.
fn decode_folder(name: &str, known: &[String]) -> (String, &'static str, bool) {
    let best = known
        .iter()
        .filter(|root| root.starts_with('/'))
        .filter_map(|root| {
            let m = crate::speaker::mangle_root(root);
            if name == m {
                Some((root.clone(), String::new()))
            } else {
                name.strip_prefix(&format!("{m}-"))
                    .map(|rest| (root.clone(), rest.to_string()))
            }
        })
        .max_by_key(|(root, _)| root.len());
    match best {
        Some((root, rest)) if rest.is_empty() => (root, "known_root", true),
        Some((root, rest)) => {
            let certain = rest.starts_with("-claude-worktrees-");
            (
                format!("{root}/{}", decode_rest(&rest)),
                "known_root",
                certain,
            )
        }
        None => match name.strip_prefix('-') {
            Some(rest) => (format!("/{}", decode_rest(rest)), "folder_name", false),
            None => (name.to_string(), "folder_name", false),
        },
    }
}

/// Whether a Claude Code project's stored root is a path a person picked, not
/// one read off its folder name.
///
/// The ledger writes a folder it found on disk with the name's every `-` made
/// `/` (`ingest`'s `demangle`), and such a folder has at least one recording
/// (`discover` takes a folder only for its `*.jsonl`). The other writer is the
/// First run screen, which stores the real path of a folder it offered before
/// anything was recorded in it. So the stored root is a pick when it differs
/// from that reading — it could not have come from the name — or when no
/// recording of this folder exists at all (`unrecorded`). Otherwise it may be
/// the name read back, and nothing can say it is not.
fn stored_root_was_picked(id: &str, stored: &str, unrecorded: bool) -> bool {
    stored.starts_with('/') && (unrecorded || stored != crate::ingest::demangle(id))
}

/// `--` → `/.`, every other `-` → `/`; a leading `-` (the rest began with
/// `/.`) is a hidden directory. After `.claude/worktrees/` the rest is ONE
/// folder — the vendor names worktrees `adjective-name-hex` — so its `-` are
/// kept.
fn decode_rest(rest: &str) -> String {
    let (head, worktree) = match rest.find("-claude-worktrees-") {
        Some(at) if at == 0 || rest[..at].ends_with('-') => {
            let name = &rest[at + "-claude-worktrees-".len()..];
            (&rest[..at], Some(name))
        }
        _ => (rest, None),
    };
    let mut decoded = head.replace("--", "/.").replace('-', "/");
    if let Some(name) = worktree {
        // `head` ended with the `-` of `--claude`, decoded above as `/`.
        decoded.push_str(".claude/worktrees/");
        decoded.push_str(name);
    }
    match decoded.strip_prefix('/') {
        // `rest` began with `-`: the separator `/` was already consumed by
        // the caller, so what follows is `.name`.
        Some(tail) => format!(".{tail}"),
        None => decoded,
    }
}

/// `path` is `root` or inside it (by whole path components).
pub fn is_under(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn worktree_parent(root: &str) -> String {
    match root.find("/.claude/worktrees/") {
        Some(i) => root[..i].to_string(),
        None => root.to_string(),
    }
}

/// The `cwd` the first lines of a transcript state (read: at most 512 KiB).
///
/// Reads line by line and stops at the first line that states one (C27: the
/// load plan calls this for every Codex recording on the machine, and reading
/// the whole 512 KiB of each was most of the plan's cost). The answer is the
/// one a whole-window read gives: the same window, the same lines, in order.
pub fn recorded_cwd(file: &Path) -> Option<String> {
    use std::io::{BufRead as _, Read as _};
    let handle = std::fs::File::open(file).ok()?;
    if !handle.metadata().ok()?.is_file() {
        return None;
    }
    let mut reader = std::io::BufReader::new(handle.take(512 * 1024));
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        let Ok(v) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        let cwd = v
            .get("cwd")
            .or_else(|| v.pointer("/payload/cwd"))
            .and_then(Value::as_str);
        if let Some(cwd) = cwd.filter(|c| c.starts_with('/')) {
            return Some(cwd.trim_end_matches('/').to_string());
        }
    }
}

/// The Codex `session_meta` facts HQ reads: its id, whether it is a
/// sub-agent, and its parent thread.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexOrigin {
    pub key: Option<String>,
    pub subagent: bool,
    pub parent: Option<String>,
}

/// Read the first line of a Codex recording (at most 8 MiB).
pub fn codex_origin(file: &Path) -> Option<CodexOrigin> {
    use std::io::{BufRead as _, Read as _};
    let handle = std::fs::File::open(file).ok()?;
    if !handle.metadata().ok()?.is_file() {
        return None;
    }
    let mut reader = std::io::BufReader::new(handle.take(8 * 1024 * 1024));
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line).ok()?;
    let v: Value = serde_json::from_slice(&line).ok()?;
    let payload = v.get("payload")?;
    let source = payload.get("source");
    let subagent = source.and_then(|s| s.get("subagent")).is_some();
    let parent = payload
        .get("parent_thread_id")
        .and_then(Value::as_str)
        .or_else(|| {
            source
                .and_then(|s| s.pointer("/subagent/thread_spawn/parent_thread_id"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);
    Some(CodexOrigin {
        key: payload
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string),
        subagent,
        parent,
    })
}

/* ───────────────────────────── rows ───────────────────────────── */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    User,
    Assistant,
    Thinking,
    ToolUse,
    ToolResult,
}

/// One command found in a row.
#[derive(Debug, Clone)]
pub(crate) struct Cmd {
    /// Index within the row (Codex programs hold several).
    pub part: usize,
    pub text: Option<String>,
    pub test: bool,
    pub commit: bool,
    pub codemode: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub id: String,
    pub session: usize,
    pub seq: i64,
    pub ts: Option<String>,
    pub ms: i64,
    pub kind: Kind,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub is_error: bool,
    pub sidechain: bool,
    /// Person's prompt text (unmasked) when this row is one.
    pub prompt: Option<String>,
    /// Assistant text (for claims and questions).
    pub text: Option<String>,
    /// Display paths of the files this row edits.
    pub files: Vec<String>,
    pub cmds: Vec<Cmd>,
    pub copied: bool,
    /// Row index of this tool_use's result.
    pub result: Option<usize>,
    pub version: usize,
}

impl Row {
    fn is_work(&self) -> bool {
        !self.files.is_empty() || !self.cmds.is_empty()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SessionInfo {
    pub id: String,
    pub ledger_project: String,
    pub source: String,
    pub file_path: String,
    pub first_ms: i64,
    pub last_ms: i64,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub origin: CodexOrigin,
    /// Rows `[start, end)` in `Index::rows`.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Version {
    pub id: String,
    pub session: usize,
    pub from_seq: i64,
    pub to_seq: i64,
    pub committed: bool,
    pub commit: Option<String>,
    /// Row index of the commit command closing it.
    pub boundary: Option<usize>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Thread {
    pub id: String,
    pub first_prompt: usize,
    /// Row indices, in thread order.
    pub rows: Vec<usize>,
    /// Person's prompt rows that started work.
    pub work_prompts: Vec<usize>,
    /// Person's prompt rows that did not.
    pub conversation: Vec<usize>,
    pub sessions: BTreeSet<usize>,
    /// Content addresses computed once at index build, after row order and
    /// membership are final. Warm Inbox reads never rehash the event list.
    pub inbox_thread_sha256: String,
    pub inbox_prompt_sha256: String,
}

/// Everything HQ knows about one project, before `now` is applied. Building
/// it is the expensive part; [`Index::view`] and [`thread`] reuse it.
#[derive(Debug, Clone)]
pub struct Index {
    pub(crate) project: HqProject,
    pub(crate) sessions: Vec<SessionInfo>,
    pub(crate) rows: Vec<Row>,
    pub(crate) threads: Vec<Thread>,
    pub(crate) versions: Vec<Version>,
    pub(crate) conversation_only: u64,
    pub(crate) unattached_turns: u64,
    pub(crate) copied: u64,
    pub(crate) claims: HashMap<usize, Vec<crate::extract::Claim>>,
    /// Result text of test and commit commands, keyed by result row.
    pub(crate) result_text: HashMap<usize, String>,
}

/// What a cache may key an [`Index`] on: each member's chain position.
pub fn fingerprint(ledger: &Ledger, project: &HqProject) -> Result<String> {
    fingerprint_on(ledger.conn(), project)
}

/// Session ids Decisions gives each terminal confirmation (`decision-terminal-`
/// then a ULID; its log is kept under `decision-confirmations/` beside the
/// ledger). A truthful record of its own, with no recording envelope: it
/// never states a `cwd`.
const TERMINAL_CONFIRMATION_SESSION: &str = "decision-terminal-";

/// The newest recordings an agent wrote for a project, newest first: every
/// session but Decisions' terminal confirmations, which state no `cwd` and so
/// must not push the transcripts that do out of the window the project's root
/// is read from (HQ-NEEDS, C28: five confirmations used to lose the root).
fn recent_native_files_on(
    conn: &Connection,
    project_id: &str,
    limit: usize,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT file_path FROM sessions
          WHERE project_id = ?1 AND substr(id, 1, ?3) <> ?4
          ORDER BY started_at DESC, id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(
        params![
            project_id,
            limit as i64,
            TERMINAL_CONFIRMATION_SESSION.len() as i64,
            TERMINAL_CONFIRMATION_SESSION
        ],
        |r| r.get::<_, String>(0),
    )?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Find one HQ project by id.
pub fn find_project(ledger: &Ledger, id: &str) -> Result<Option<HqProject>> {
    Ok(hq_projects(ledger)?.into_iter().find(|p| p.id == id))
}

/// [`find_project`] on any connection to a ledger. It keeps the native
/// recorded-cwd and multi-member grouping of [`find_project`] exactly (it is the
/// same derivation) and reads through the connection it is given, so a caller
/// may hand it one opened `SQLITE_OPEN_READ_ONLY` and the whole lookup stays
/// physically read-only (HQ-NEEDS, C28: read-only HQ project lookup for inline
/// HTTP). It opens nothing, creates nothing and appends no row.
pub fn find_project_on(conn: &Connection, id: &str) -> Result<Option<HqProject>> {
    Ok(hq_projects_on(conn)?.into_iter().find(|p| p.id == id))
}

/// Build the index for one HQ project.
pub fn index(ledger: &Ledger, project: &HqProject) -> Result<Index> {
    index_on(ledger.conn(), project)
}

/// [`index`] on any connection to a ledger. Reads only; a measurement may
/// pass a connection it opened read-only.
pub fn index_on(conn: &Connection, project: &HqProject) -> Result<Index> {
    // One reader per `SESSIONS_PER_READER` sessions, up to
    // `PARALLEL_READERS`: a small project is read on the caller's connection
    // and opens nothing.
    let marks = vec!["?"; project.members.len()].join(",");
    let sessions: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM sessions WHERE project_id IN ({marks})"),
        params_from_iter(project.members.iter().map(|m| &m.ledger_project_id)),
        |r| r.get(0),
    )?;
    let workers = usize::try_from(sessions)
        .unwrap_or(0)
        .div_ceil(SESSIONS_PER_READER)
        .min(PARALLEL_READERS);
    let read = if workers <= 1 {
        RowRead::OneStatement
    } else {
        RowRead::Parallel {
            workers,
            rowid_order: true,
            runs_per_reader: RUNS_PER_READER,
        }
    };
    index_with(conn, project, read)
}

/// A few sessions hold most rows, so the sessions are cut into this many
/// runs per reader, taken in turn, rather than one equal share each.
pub const RUNS_PER_READER: usize = 4;

/// See [`index_on`].
const SESSIONS_PER_READER: usize = 8;

/// How [`index_with`] reads the project's rows. Every way yields the same
/// rows in the same order, so the same index (C27 (a)).
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowRead {
    /// One statement on the caller's connection, in `(session_id, seq)`
    /// order. The reference; also what a ledger with no file (in memory)
    /// is read with.
    OneStatement,
    /// `workers` read-only connections to the same file, each in its own
    /// read transaction, taking the sessions in contiguous runs of the
    /// session order. Several reads are in flight at once, so a cold store
    /// is read with a deeper queue. Each run's rows are read in rowid order
    /// (`rowid_order`), which follows the file's own layout, and put back in
    /// `(session_id, seq)` order before they are used. The sessions are cut
    /// into `workers × runs_per_reader` runs.
    Parallel {
        workers: usize,
        rowid_order: bool,
        runs_per_reader: usize,
    },
}

/// The most read-only connections [`index_on`] opens. Measured on the
/// founder's store (C27 (a), DECISIONS.md): a cold read took 9.1 s on one
/// connection, 5.6 s on 4, 3.2–3.4 s on 8, 2.3 s on 16 and 2.3 s on 32.
pub const PARALLEL_READERS: usize = 16;

/// One recorded row as [`index_with`] reads it.
type Raw = (
    String,
    String,
    i64,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    bool,
    Option<String>,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// One session row: id, ledger project, file path, source.
type SessionRow = (String, String, String, String);

/// The columns read for each row. `is_error`, `src_uuid` and `is_sidechain`
/// sit after a result's (often large) `tool_result`, so they are not read for
/// results.
const ROW_COLUMNS: &str = "id, session_id, seq, ts, kind, tool_name, tool_use_id,
                CASE WHEN kind <> 'tool_result' THEN is_error END,
                CASE WHEN kind <> 'tool_result' THEN src_uuid END,
                CASE WHEN kind <> 'tool_result' THEN is_sidechain END,
                CASE kind WHEN 'user_msg' THEN substr(text, 1, 8000)
                          WHEN 'assistant_text' THEN text END,
                CASE WHEN kind = 'tool_use' AND json_valid(tool_input)
                      AND (tool_name IN ('Write','MultiEdit','NotebookEdit')
                           OR (tool_name = 'Edit' AND source_tool_name IS NULL))
                     THEN COALESCE(json_extract(tool_input, '$.file_path'),
                                   json_extract(tool_input, '$.notebook_path')) END,
                CASE WHEN kind = 'tool_use'
                      AND (tool_name IN ('Bash', 'exec')
                           OR (tool_name = 'Edit' AND source_tool_name IS NOT NULL))
                     THEN tool_input END";

const ROW_KINDS: &str =
    "kind IN ('user_msg','assistant_text','assistant_thinking','tool_use','tool_result')";

fn raw_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Raw> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get::<_, Option<i64>>(7)?.unwrap_or(0) != 0,
        r.get(8)?,
        r.get::<_, Option<i64>>(9)?.unwrap_or(0) != 0,
        r.get(10)?,
        r.get(11)?,
        r.get(12)?,
    ))
}

fn session_rows(conn: &Connection, ids: &[String]) -> Result<Vec<SessionRow>> {
    let marks = vec!["?"; ids.len()].join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT s.id, s.project_id, s.file_path, p.source FROM sessions s
         JOIN projects p ON p.id = s.project_id
         WHERE s.project_id IN ({marks}) ORDER BY s.id"
    ))?;
    let found = stmt
        .query_map(params_from_iter(ids.iter()), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(found)
}

/// The reference read: one statement, `(session_id, seq)` order.
fn rows_one_statement(conn: &Connection, ids: &[String]) -> Result<Vec<Raw>> {
    let marks = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT {ROW_COLUMNS}
         FROM events
         WHERE session_id IN (SELECT id FROM sessions WHERE project_id IN ({marks}))
           AND {ROW_KINDS}
         ORDER BY session_id, seq"
    );
    let mut stmt = conn.prepare(&sql)?;
    let raws = stmt
        .query_map(params_from_iter(ids.iter()), raw_of)?
        .collect::<std::result::Result<_, _>>()?;
    Ok(raws)
}

/// The rows of one contiguous run of sessions, in `(session_id, seq)` order.
fn rows_of_sessions(conn: &Connection, sessions: &[String], rowid_order: bool) -> Result<Vec<Raw>> {
    let marks = vec!["?"; sessions.len()].join(",");
    let sql = if rowid_order {
        format!(
            "SELECT {ROW_COLUMNS} FROM events
             WHERE rowid IN (SELECT rowid FROM events WHERE session_id IN ({marks}))
               AND {ROW_KINDS}"
        )
    } else {
        format!(
            "SELECT {ROW_COLUMNS} FROM events
             WHERE session_id IN ({marks}) AND {ROW_KINDS}
             ORDER BY session_id, seq"
        )
    };
    let mut stmt = conn.prepare(&sql)?;
    let mut raws: Vec<Raw> = stmt
        .query_map(params_from_iter(sessions.iter()), raw_of)?
        .collect::<std::result::Result<_, _>>()?;
    if rowid_order {
        // `(session_id, seq)` is unique, so this order is total; byte order
        // of the id is SQLite's BINARY collation.
        raws.sort_unstable_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
    }
    Ok(raws)
}

/// The members' chain positions as one connection's snapshot sees them.
fn fingerprint_on(conn: &Connection, project: &HqProject) -> Result<String> {
    let mut parts = Vec::new();
    for m in &project.members {
        let total: Option<(String, i64)> = conn
            .query_row(
                "SELECT digest, event_total FROM project_heads WHERE project_id = ?1",
                [&m.ledger_project_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE project_id = ?1",
            [&m.ledger_project_id],
            |r| r.get(0),
        )?;
        parts.push(format!(
            "{}:{}:{}:{count}",
            m.ledger_project_id,
            total.as_ref().map_or("", |t| t.0.as_str()),
            total.as_ref().map_or(0, |t| t.1)
        ));
    }
    Ok(sha256_hex(parts.join("\n").as_bytes()))
}

/// A read-only connection to `path`, inside a read transaction whose
/// snapshot is pinned by reading the members' fingerprint, which it returns.
/// Never `immutable`: hooks write the store while it is read.
fn snapshot_reader(path: &str, project: &HqProject) -> Result<(Connection, String)> {
    let db = Path::new(path);
    let conn = crate::store_hold::open_connection(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("BEGIN DEFERRED")?;
    let fp = fingerprint_on(&conn, project)?;
    Ok((conn, fp))
}

/// How many times opening the readers is tried again when a write landed
/// while they were starting, before the read falls back to one statement.
const SNAPSHOT_TRIES: usize = 3;

/// Read-only connections to one store that all see one snapshot of it: each
/// one's read transaction starts by reading the members' fingerprint, and a
/// set whose fingerprints differ is thrown away and opened again.
struct Readers {
    conns: Vec<Connection>,
}

impl Readers {
    /// `n` readers on one snapshot, or `None` when writes kept landing.
    fn open(path: &str, project: &HqProject, n: usize) -> Result<Option<Readers>> {
        for _ in 0..SNAPSHOT_TRIES {
            let opened: Vec<Result<(Connection, String)>> = std::thread::scope(|scope| {
                let handles: Vec<_> = (0..n.max(1))
                    .map(|_| scope.spawn(|| snapshot_reader(path, project)))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| match h.join() {
                        Ok(r) => r,
                        Err(e) => std::panic::resume_unwind(e),
                    })
                    .collect()
            });
            let mut conns = Vec::with_capacity(opened.len());
            let mut seen: Option<String> = None;
            let mut one_snapshot = true;
            for o in opened {
                let (conn, fp) = o?;
                match &seen {
                    None => seen = Some(fp),
                    Some(first) => one_snapshot &= *first == fp,
                }
                conns.push(conn);
            }
            if one_snapshot {
                return Ok(Some(Readers { conns }));
            }
        }
        Ok(None)
    }

    /// Jobs `0..jobs`, taken in turn by the readers, so several reads are in
    /// flight at once. Results come back in job order; the first error wins.
    fn run<T: Send>(
        &mut self,
        jobs: usize,
        f: impl Fn(&Connection, usize) -> Result<T> + Sync,
    ) -> Result<Vec<T>> {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::Mutex;
        let slots: Vec<Mutex<Option<T>>> = (0..jobs).map(|_| Mutex::new(None)).collect();
        let next = AtomicUsize::new(0);
        let failed = AtomicBool::new(false);
        let conns = std::mem::take(&mut self.conns);
        let (back, outcome): (Vec<Connection>, Result<()>) = std::thread::scope(|scope| {
            let handles: Vec<_> = conns
                .into_iter()
                .map(|conn| {
                    let (f, slots, next, failed) = (&f, &slots, &next, &failed);
                    scope.spawn(move || {
                        let outcome = loop {
                            if failed.load(Ordering::Relaxed) {
                                break Ok(());
                            }
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            if i >= jobs {
                                break Ok(());
                            }
                            match f(&conn, i) {
                                Ok(v) => {
                                    if let Ok(mut slot) = slots[i].lock() {
                                        *slot = Some(v);
                                    }
                                }
                                Err(e) => {
                                    failed.store(true, Ordering::Relaxed);
                                    break Err(e);
                                }
                            }
                        };
                        (conn, outcome)
                    })
                })
                .collect();
            let mut back = Vec::new();
            let mut outcome = Ok(());
            for h in handles {
                match h.join() {
                    Ok((conn, o)) => {
                        back.push(conn);
                        if outcome.is_ok() {
                            outcome = o;
                        }
                    }
                    Err(e) => std::panic::resume_unwind(e),
                }
            }
            (back, outcome)
        });
        self.conns = back;
        outcome?;
        slots
            .into_iter()
            .map(|slot| {
                slot.into_inner().ok().flatten().ok_or_else(|| {
                    crate::error::CoreError::Integrity("a parallel read left a job unread".into())
                })
            })
            .collect()
    }
}

/// What is read off one row's own columns, pure per row: the files a call
/// edits and the commands it runs (JSON, Codex programs, command segments),
/// and the claims of finished work an agent's words make.
type Digest = ((Vec<String>, Vec<Cmd>), Vec<crate::extract::Claim>);

fn digest(raw: &Raw, roots: &[String]) -> Digest {
    let work = if raw.4 == "tool_use" {
        work_of(raw.5.as_deref(), raw.11.clone(), raw.12.as_deref(), roots)
    } else {
        (Vec::new(), Vec::new())
    };
    let claims = match (&raw.10, raw.4 == "assistant_text") {
        (Some(text), true) => MarkerExtractor
            .extract(text)
            .into_iter()
            .filter(|c| matches!(c.kind, ClaimKind::Work | ClaimKind::Marker))
            .collect(),
        _ => Vec::new(),
    };
    (work, claims)
}

/// Where [`index_with`] reads: one connection, or a set of [`Readers`].
enum Source<'a> {
    One(&'a Connection),
    Many {
        readers: Readers,
        rowid_order: bool,
        runs_per_reader: usize,
    },
}

impl Source<'_> {
    fn sessions(&self, ids: &[String]) -> Result<Vec<SessionRow>> {
        match self {
            Source::One(conn) => session_rows(conn, ids),
            Source::Many { readers, .. } => match readers.conns.first() {
                Some(conn) => session_rows(conn, ids),
                None => Ok(Vec::new()),
            },
        }
    }

    /// Every row, in `(session_id, seq)` order, each with its [`digest`].
    fn rows(
        &mut self,
        ids: &[String],
        sessions: &[SessionRow],
        roots: &[String],
    ) -> Result<Vec<(Raw, Digest)>> {
        match self {
            Source::One(conn) => {
                let raws = rows_one_statement(conn, ids)?;
                let digests = parallel_map(&raws, |raw| digest(raw, roots));
                Ok(raws.into_iter().zip(digests).collect())
            }
            Source::Many {
                readers,
                rowid_order,
                runs_per_reader,
            } => {
                let names: Vec<String> = sessions.iter().map(|s| s.0.clone()).collect();
                // Runs of the session order, taken in turn. Runs are
                // contiguous, so their rows joined in run order are in
                // `(session_id, seq)` order.
                let size = names
                    .len()
                    .div_ceil(readers.conns.len().max(1) * (*runs_per_reader).max(1))
                    .max(1);
                let runs: Vec<&[String]> = names.chunks(size).collect();
                let order = *rowid_order;
                // Each reader digests its own rows while the others' reads
                // are in flight.
                let parts = readers.run(runs.len(), |conn, i| {
                    let raws = rows_of_sessions(conn, runs[i], order)?;
                    Ok(raws
                        .into_iter()
                        .map(|raw| {
                            let d = digest(&raw, roots);
                            (raw, d)
                        })
                        .collect::<Vec<_>>())
                })?;
                Ok(parts.into_iter().flatten().collect())
            }
        }
    }

    /// `f` over jobs `0..jobs` on this source's connection(s), in job order.
    fn each<T: Send>(
        &mut self,
        jobs: usize,
        f: impl Fn(&Connection, usize) -> Result<T> + Sync,
    ) -> Result<Vec<T>> {
        match self {
            Source::One(conn) => (0..jobs).map(|i| f(conn, i)).collect(),
            Source::Many { readers, .. } => readers.run(jobs, f),
        }
    }
}

/// [`index`], reading rows the given way. Every [`RowRead`] gives the same
/// index; this is public so tests and the probe can compare them.
#[doc(hidden)]
pub fn index_with(conn: &Connection, project: &HqProject, read: RowRead) -> Result<Index> {
    let ids: Vec<String> = project
        .members
        .iter()
        .map(|m| m.ledger_project_id.clone())
        .collect();

    // A ledger in memory has no file another connection could open.
    let file = conn.path().filter(|p| !p.is_empty());
    let readers = match (read, file) {
        (
            RowRead::Parallel {
                workers,
                rowid_order,
                runs_per_reader,
            },
            Some(path),
        ) => Readers::open(path, project, workers)?.map(|readers| Source::Many {
            readers,
            rowid_order,
            runs_per_reader,
        }),
        _ => None,
    };
    let mut source = readers.unwrap_or(Source::One(conn));
    let found = source.sessions(&ids)?;
    let roots: Vec<String> = std::iter::once(project.root.clone())
        .chain(project.members.iter().map(|m| m.root.clone()))
        .collect();
    let raws = source.rows(&ids, &found, &roots)?;

    // Sessions.
    let mut sessions: Vec<SessionInfo> = Vec::new();
    for (id, ledger_project, file_path, source) in found {
        let origin = if source == "codex" {
            codex_origin(Path::new(&file_path)).unwrap_or_default()
        } else {
            CodexOrigin::default()
        };
        sessions.push(SessionInfo {
            id,
            ledger_project,
            source,
            file_path,
            first_ms: i64::MAX,
            last_ms: i64::MIN,
            first_ts: None,
            last_ts: None,
            origin,
            start: 0,
            end: 0,
        });
    }
    let session_index: HashMap<String, usize> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();

    let mut rows: Vec<Row> = Vec::with_capacity(raws.len());
    let mut uuids: Vec<Option<String>> = Vec::with_capacity(raws.len());
    let mut row_claims: Vec<Vec<crate::extract::Claim>> = Vec::with_capacity(raws.len());
    let mut current: Option<usize> = None;
    let mut last_ms = 0i64;
    for (raw, ((files, cmds), claims)) in raws {
        let (
            id,
            sid,
            seq,
            ts,
            kind,
            tool_name,
            tool_use_id,
            is_error,
            src_uuid,
            sidechain,
            text,
            _file,
            _input,
        ) = raw;
        let Some(&s) = session_index.get(&sid) else {
            continue;
        };
        if current != Some(s) {
            if let Some(prev) = current {
                sessions[prev].end = rows.len();
            }
            sessions[s].start = rows.len();
            current = Some(s);
            last_ms = 0;
        }
        let kind = match kind.as_str() {
            "user_msg" => Kind::User,
            "assistant_text" => Kind::Assistant,
            "assistant_thinking" => Kind::Thinking,
            "tool_use" => Kind::ToolUse,
            _ => Kind::ToolResult,
        };
        let ms = ts.as_deref().and_then(parse_ms).unwrap_or(last_ms);
        last_ms = ms;
        let (prompt, text) = match kind {
            Kind::User => (text, None),
            Kind::Assistant => (None, text),
            _ => (None, None),
        };
        rows.push(Row {
            id,
            session: s,
            seq,
            ts,
            ms,
            kind,
            tool_name,
            tool_use_id,
            is_error,
            sidechain,
            prompt,
            text,
            files,
            cmds,
            copied: false,
            result: None,
            version: 0,
        });
        uuids.push(src_uuid);
        row_claims.push(claims);
    }
    if let Some(prev) = current {
        sessions[prev].end = rows.len();
    }
    for s in sessions.iter_mut() {
        for r in &rows[s.start..s.end] {
            if r.ts.is_some() {
                if s.first_ts.is_none() {
                    s.first_ts = r.ts.clone();
                    s.first_ms = r.ms;
                }
                s.last_ts = r.ts.clone();
                s.last_ms = s.last_ms.max(r.ms);
            }
        }
        if s.first_ms == i64::MAX {
            s.first_ms = 0;
            s.last_ms = 0;
        }
    }

    // Results.
    let mut result_of: HashMap<String, usize> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        if r.kind == Kind::ToolResult {
            if let Some(t) = &r.tool_use_id {
                result_of.entry(t.clone()).or_insert(i);
            }
        }
    }
    for r in rows.iter_mut() {
        if r.kind == Kind::ToolUse {
            r.result = r
                .tool_use_id
                .as_ref()
                .and_then(|t| result_of.get(t).copied());
        }
    }

    // A result's `is_error` sits after its (often large) `tool_result` in the
    // row, so reading it in the scan above would read every result's bytes.
    // It is read here, by primary key, only for the calls whose outcome is
    // served from it (Claude Code and classic Codex calls, not programs).
    {
        let wanted: Vec<usize> = rows
            .iter()
            .filter(|r| {
                r.kind == Kind::ToolUse && r.tool_name.as_deref() != Some("exec") && r.is_work()
            })
            .filter_map(|r| r.result)
            .collect();
        let chunks: Vec<&[usize]> = wanted.chunks(400).collect();
        let rows_ref = &rows;
        let found: Vec<HashMap<String, bool>> = source.each(chunks.len(), |conn, c| {
            let chunk = chunks[c];
            let marks = vec!["?"; chunk.len()].join(",");
            let mut stmt = conn.prepare(&format!(
                "SELECT id, is_error FROM events WHERE id IN ({marks})"
            ))?;
            let found: HashMap<String, bool> = stmt
                .query_map(
                    params_from_iter(chunk.iter().map(|&i| rows_ref[i].id.clone())),
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)),
                )?
                .collect::<std::result::Result<_, _>>()?;
            Ok(found)
        })?;
        for (chunk, found) in chunks.iter().zip(found) {
            for &i in chunk.iter() {
                rows[i].is_error = found.get(&rows[i].id).copied().unwrap_or(false);
            }
        }
    }

    // Session order.
    let mut order: Vec<usize> = (0..sessions.len()).collect();
    order.sort_by(|a, b| {
        sessions[*a]
            .first_ms
            .cmp(&sessions[*b].first_ms)
            .then_with(|| sessions[*a].id.cmp(&sessions[*b].id))
    });

    // Rule 1: copied history.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut copied = 0u64;
    for &s in &order {
        for i in sessions[s].start..sessions[s].end {
            if let Some(u) = &uuids[i] {
                match seen.get(u) {
                    Some(&orig) if rows[orig].session != s => {
                        rows[i].copied = true;
                        copied += 1;
                    }
                    Some(_) => {}
                    None => {
                        seen.insert(u.clone(), i);
                    }
                }
            }
        }
    }

    // Result text for test and commit commands (by primary key; few rows).
    let mut result_text: HashMap<usize, String> = HashMap::new();
    {
        let wanted: Vec<usize> = rows
            .iter()
            .filter(|r| r.cmds.iter().any(|c| c.test || c.commit))
            .filter_map(|r| r.result)
            .collect();
        let chunks: Vec<&[usize]> = wanted.chunks(32).collect();
        let rows_ref = &rows;
        let texts: Vec<Vec<String>> = source.each(chunks.len(), |conn, c| {
            Ok(chunks[c]
                .iter()
                .map(|&ri| {
                    let raw: Option<String> = conn
                        .query_row(
                            "SELECT tool_result FROM events WHERE id = ?1",
                            [&rows_ref[ri].id],
                            |x| x.get(0),
                        )
                        .ok()
                        .flatten();
                    raw.and_then(|t| serde_json::from_str::<Value>(&t).ok())
                        .map(|v| commands::result_text(&v))
                        .unwrap_or_default()
                })
                .collect())
        })?;
        for (ri, text) in wanted.into_iter().zip(texts.into_iter().flatten()) {
            result_text.insert(ri, text);
        }
    }
    // Nothing below reads through the readers; end their read transactions
    // now rather than at the end of the build.
    drop(source);

    // Versions, per session. A later edit changes the version a test examined,
    // even when the agent makes no Git commit between the two events.
    let mut versions: Vec<Version> = Vec::new();
    for (s, info) in sessions.iter().enumerate() {
        if info.start == info.end {
            continue;
        }
        let mut from = rows[info.start].seq;
        let mut members: Vec<usize> = Vec::new();
        let mut tested_since_boundary = false;
        let mut included_commit_result_through: Option<usize> = None;
        for i in info.start..info.end {
            if included_commit_result_through.is_some_and(|end| i <= end) {
                continue;
            }
            if !rows[i].copied && !rows[i].files.is_empty() && tested_since_boundary {
                let v = versions.len();
                versions.push(Version {
                    id: wt_id(&info.id, from),
                    session: s,
                    from_seq: from,
                    to_seq: rows[i - 1].seq,
                    committed: false,
                    commit: None,
                    boundary: None,
                    message: None,
                });
                for m in members.drain(..) {
                    rows[m].version = v;
                }
                from = rows[i].seq;
                tested_since_boundary = false;
            }
            members.push(i);
            if !rows[i].copied && rows[i].cmds.iter().any(|c| c.test) {
                tested_since_boundary = true;
            }
            let (seq, hash, message, result_row) = {
                let r = &rows[i];
                if r.copied || !r.cmds.iter().any(|c| c.commit) {
                    continue;
                }
                let rt = r
                    .result
                    .and_then(|ri| result_text.get(&ri))
                    .map(String::as_str);
                let hash = rt.and_then(commands::commit_hash);
                let took = hash.is_some()
                    || outcome_of(&rows, r, r.cmds.iter().find(|c| c.commit), &result_text)
                        == Outcome::Succeeded;
                if !took {
                    continue;
                }
                let message = r
                    .cmds
                    .iter()
                    .find(|c| c.commit)
                    .and_then(|c| c.text.as_deref())
                    .and_then(commands::commit_message);
                // The tool result confirms the commit and carries its hash.
                // Keep that result in the committed version when no other work
                // was recorded between the use and its result. Otherwise an
                // empty trailing worktree version becomes the current one.
                let result_row = r.result.filter(|&ri| {
                    ri > i
                        && ri < info.end
                        && rows[i + 1..ri].iter().all(|between| !between.is_work())
                });
                (r.seq, hash, message, result_row)
            };
            let through_seq = result_row.map(|ri| rows[ri].seq).unwrap_or(seq);
            if let Some(ri) = result_row {
                members.extend(i + 1..=ri);
                included_commit_result_through = Some(ri);
            }
            let v = versions.len();
            versions.push(Version {
                id: hash
                    .as_deref()
                    .map(|h| h.chars().take(7).collect())
                    .unwrap_or_else(|| wt_id(&info.id, from)),
                session: s,
                from_seq: from,
                to_seq: through_seq,
                committed: true,
                commit: hash,
                boundary: Some(i),
                message,
            });
            for m in members.drain(..) {
                rows[m].version = v;
            }
            from = through_seq + 1;
            tested_since_boundary = false;
        }
        if !members.is_empty() {
            let v = versions.len();
            versions.push(Version {
                id: wt_id(&info.id, from),
                session: s,
                from_seq: from,
                to_seq: rows[info.end - 1].seq,
                committed: false,
                commit: None,
                boundary: None,
                message: None,
            });
            for m in members {
                rows[m].version = v;
            }
        }
    }

    // Prompt classification, per session. The expanded body Claude Code
    // writes after a slash-command wrapper lands within the same instant; a
    // person's next prompt does not, so only a user row within
    // `WRAPPER_BODY_MS` of a wrapper is taken as its body.
    for info in &sessions {
        let agent_session = info.origin.subagent;
        let mut wrapper_at: Option<i64> = None;
        #[allow(clippy::needless_range_loop)] // rows[i] is read and written
        for i in info.start..info.end {
            if rows[i].kind != Kind::User {
                wrapper_at = None;
                continue;
            }
            let text = rows[i].prompt.take().unwrap_or_default();
            if rows[i].copied || rows[i].sidechain || agent_session {
                wrapper_at = None;
                continue;
            }
            let after_wrapper =
                wrapper_at.is_some_and(|w| (rows[i].ms - w).abs() <= WRAPPER_BODY_MS);
            match prompts::classify(&text, after_wrapper) {
                prompts::PromptClass::Human { text } => {
                    rows[i].prompt = Some(text);
                    wrapper_at = None;
                }
                prompts::PromptClass::CommandWrapper => wrapper_at = Some(rows[i].ms),
                prompts::PromptClass::Injected => wrapper_at = None,
            }
        }
    }

    // Threads.
    let mut builder = Builder::default();
    let mut session_last: Vec<Option<usize>> = vec![None; sessions.len()];
    let mut row_thread: Vec<Option<usize>> = vec![None; rows.len()];
    let mut conversation_only = 0u64;
    let mut unattached = 0u64;
    let mut subagents: Vec<usize> = Vec::new();
    for &s in &order {
        let info = &sessions[s];
        if info.origin.subagent {
            subagents.push(s);
            continue;
        }
        // Rule 1/2: what is open when this session starts.
        let mut open: Option<usize> = None;
        for i in info.start..info.end {
            if rows[i].copied {
                if let Some(t) = uuids[i]
                    .as_ref()
                    .and_then(|u| seen.get(u))
                    .and_then(|o| row_thread[*o])
                {
                    open = Some(t);
                }
            }
        }
        if open.is_none() {
            let first_user =
                (info.start..info.end).find(|&i| rows[i].kind == Kind::User && !rows[i].sidechain);
            let continued = first_user.is_some_and(|i| starts_continuation(conn, &rows[i].id));
            if continued {
                let pred = order
                    .iter()
                    .take_while(|&&p| p != s)
                    .filter(|&&p| {
                        !sessions[p].origin.subagent && sessions[p].last_ms <= info.first_ms
                    })
                    .max_by_key(|&&p| sessions[p].last_ms);
                if let Some(&p) = pred {
                    open = session_last[p];
                }
            }
        }
        // Turns.
        let mut turns: Vec<(Option<usize>, Vec<usize>)> = vec![(None, Vec::new())];
        for (i, row) in rows.iter().enumerate().take(info.end).skip(info.start) {
            if row.copied {
                continue;
            }
            if row.prompt.is_some() {
                turns.push((Some(i), vec![i]));
            } else if let Some(last) = turns.last_mut() {
                last.1.push(i);
            }
        }
        for (prompt, members) in turns {
            if members.is_empty() {
                continue;
            }
            let work = members.iter().any(|&i| rows[i].is_work());
            let files: BTreeSet<String> = members
                .iter()
                .flat_map(|&i| rows[i].files.iter().cloned())
                .collect();
            let first_ms = rows[members[0]].ms;
            match prompt {
                None => match open {
                    Some(t) => builder.attach(t, &members, None, false, &rows, &mut row_thread),
                    None => {
                        if work {
                            unattached += 1;
                        }
                    }
                },
                Some(p) if !work => match open {
                    Some(t) => builder.attach(t, &members, Some(p), false, &rows, &mut row_thread),
                    None => conversation_only += 1,
                },
                Some(p) => {
                    let split = match open {
                        None => true,
                        Some(t) => {
                            let th = &builder.threads[t];
                            let disjoint = !files.is_empty() && th.files.is_disjoint(&files);
                            disjoint && (th.has_commit || first_ms - th.last_ms > SPLIT_GAP_MS)
                        }
                    };
                    let t = if split {
                        builder.open(p)
                    } else {
                        open.unwrap_or_else(|| builder.open(p))
                    };
                    builder.attach(t, &members, Some(p), true, &rows, &mut row_thread);
                    open = Some(t);
                }
            }
        }
        session_last[s] = open;
    }

    // Rule 4: sub-agent sessions join their parent's thread.
    let key_to_session: HashMap<String, usize> = sessions
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.origin.key.clone().map(|k| (k, i)))
        .collect();
    for s in subagents {
        let info = &sessions[s];
        let members: Vec<usize> = (info.start..info.end)
            .filter(|&i| !rows[i].copied)
            .collect();
        if members.is_empty() {
            continue;
        }
        let target = info
            .origin
            .parent
            .as_ref()
            .and_then(|p| key_to_session.get(p))
            .and_then(|&ps| {
                let pinfo = &sessions[ps];
                (pinfo.start..pinfo.end)
                    .filter(|&i| rows[i].ms <= info.first_ms)
                    .filter_map(|i| row_thread[i])
                    .next_back()
                    .or(session_last[ps])
            });
        match target {
            Some(t) => builder.attach(t, &members, None, false, &rows, &mut row_thread),
            None => {
                if members.iter().any(|&i| rows[i].is_work()) {
                    unattached += 1;
                }
            }
        }
    }

    // Rule 5: merge across sessions.
    let n = builder.threads.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let next = p[c];
            p[c] = r;
            c = next;
        }
        r
    }
    let mut by_start: Vec<usize> = (0..n).collect();
    by_start.sort_by_key(|&t| (builder.threads[t].first_ms, t));
    for (ai, &a) in by_start.iter().enumerate() {
        let ta = &builder.threads[a];
        if ta.files.is_empty() {
            continue;
        }
        for &b in &by_start[ai + 1..] {
            let tb = &builder.threads[b];
            if tb.first_ms > ta.last_ms.saturating_add(MERGE_WINDOW_MS) {
                break;
            }
            if tb.files.is_empty() || !ta.sessions.is_disjoint(&tb.sessions) {
                continue;
            }
            let inter = ta.files.intersection(&tb.files).count() as f64;
            let union = ta.files.union(&tb.files).count() as f64;
            if union > 0.0 && inter / union >= MERGE_JACCARD {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
    }
    let mut merged: BTreeMap<usize, Thread> = BTreeMap::new();
    for t in 0..n {
        let root = find(&mut parent, t);
        let b = &builder.threads[t];
        let entry = merged.entry(root).or_default();
        entry.rows.extend(b.rows.iter().copied());
        entry.work_prompts.extend(b.work_prompts.iter().copied());
        entry.conversation.extend(b.conversation.iter().copied());
        entry.sessions.extend(b.sessions.iter().copied());
    }
    let session_rank: Vec<usize> = {
        let mut rank = vec![0; sessions.len()];
        for (i, &s) in order.iter().enumerate() {
            rank[s] = i;
        }
        rank
    };
    let key = |i: usize| (rows[i].ms, session_rank[rows[i].session], rows[i].seq);
    let mut threads: Vec<Thread> = merged
        .into_values()
        .filter(|t| !t.work_prompts.is_empty())
        .map(|mut t| {
            t.rows.sort_by_key(|&i| key(i));
            t.rows.dedup();
            t.work_prompts.sort_by_key(|&i| key(i));
            t.conversation.sort_by_key(|&i| key(i));
            t.first_prompt = t.work_prompts[0];
            t.id = sha256_hex(rows[t.first_prompt].id.as_bytes())[..16].to_string();
            let event_ids =
                crate::inbox_hide::event_ids_sha256(t.rows.iter().map(|&i| rows[i].id.as_str()));
            t.inbox_thread_sha256 = crate::inbox_hide::row_sha256_from_events(
                crate::inbox_hide::RowKind::Thread,
                &event_ids,
            );
            t.inbox_prompt_sha256 = crate::inbox_hide::row_sha256_from_events(
                crate::inbox_hide::RowKind::Prompt,
                &event_ids,
            );
            t
        })
        .collect();
    threads.sort_by(|a, b| a.id.cmp(&b.id));

    // Claims: read off each agent row as it was read ([`digest`]); a copy of
    // an earlier session's row claims nothing.
    let claims: HashMap<usize, Vec<crate::extract::Claim>> = row_claims
        .into_iter()
        .enumerate()
        .filter(|(i, c)| !c.is_empty() && !rows[*i].copied)
        .collect();

    Ok(Index {
        project: project.clone(),
        sessions,
        rows,
        threads,
        versions,
        conversation_only,
        unattached_turns: unattached,
        copied,
        claims,
        result_text,
    })
}

/// `f` over `items` on up to 8 threads, results in input order.
fn parallel_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 8);
    if threads == 1 || items.len() < 1024 {
        return items.iter().map(&f).collect();
    }
    let chunk = items.len().div_ceil(threads);
    let f = &f;
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| scope.spawn(move || part.iter().map(f).collect::<Vec<R>>()))
            .collect();
        let mut out = Vec::with_capacity(items.len());
        for h in handles {
            match h.join() {
                Ok(part) => out.extend(part),
                // A worker cannot panic on this input (every function it
                // runs is total); if one did, resume the panic rather than
                // serve a partial answer.
                Err(e) => std::panic::resume_unwind(e),
            }
        }
        out
    })
}

fn wt_id(session: &str, from_seq: i64) -> String {
    format!(
        "wt-{}",
        &sha256_hex(format!("{session}:{from_seq}").as_bytes())[..16]
    )
}

fn starts_continuation(conn: &rusqlite::Connection, event_id: &str) -> bool {
    conn.query_row(
        "SELECT substr(text, 1, 80) FROM events WHERE id = ?1",
        [event_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .is_some_and(|t| t.starts_with(prompts::CONTINUATION_PREFIX))
}

#[derive(Default)]
struct Building {
    rows: Vec<usize>,
    work_prompts: Vec<usize>,
    conversation: Vec<usize>,
    sessions: BTreeSet<usize>,
    files: BTreeSet<String>,
    has_commit: bool,
    first_ms: i64,
    last_ms: i64,
}

#[derive(Default)]
struct Builder {
    threads: Vec<Building>,
}

impl Builder {
    fn open(&mut self, _prompt: usize) -> usize {
        self.threads.push(Building {
            first_ms: i64::MAX,
            last_ms: i64::MIN,
            ..Building::default()
        });
        self.threads.len() - 1
    }

    fn attach(
        &mut self,
        t: usize,
        members: &[usize],
        prompt: Option<usize>,
        work: bool,
        rows: &[Row],
        row_thread: &mut [Option<usize>],
    ) {
        let th = &mut self.threads[t];
        if let Some(p) = prompt {
            if work {
                th.work_prompts.push(p);
            } else {
                th.conversation.push(p);
            }
        }
        for &i in members {
            th.rows.push(i);
            th.sessions.insert(rows[i].session);
            th.files.extend(rows[i].files.iter().cloned());
            th.has_commit |= rows[i].cmds.iter().any(|c| c.commit);
            th.first_ms = th.first_ms.min(rows[i].ms);
            th.last_ms = th.last_ms.max(rows[i].ms);
            row_thread[i] = Some(t);
        }
    }
}

/// The files and commands one `tool_use` row holds.
fn work_of(
    tool_name: Option<&str>,
    file: Option<String>,
    input: Option<&str>,
    roots: &[String],
) -> (Vec<String>, Vec<Cmd>) {
    let tool = tool_name.unwrap_or("");
    if edits::EDIT_TOOLS.contains(&tool) {
        if let Some(f) = file {
            return (vec![display_path(&f, roots)], Vec::new());
        }
        // Codex classic patch.
        let Some(v) = input.and_then(|i| serde_json::from_str::<Value>(i).ok()) else {
            return (Vec::new(), Vec::new());
        };
        let text = v
            .get("patch")
            .or_else(|| v.get("input"))
            .and_then(Value::as_str)
            .unwrap_or("");
        return (
            edits::paths_of_patch(text)
                .iter()
                .map(|p| display_path(p, roots))
                .collect(),
            Vec::new(),
        );
    }
    if tool == "Bash" {
        let text = input
            .and_then(|i| serde_json::from_str::<Value>(i).ok())
            .and_then(|v| commands::command_of(&v));
        let (test, commit) = text
            .as_deref()
            .map(|t| (commands::is_test(t), commands::is_commit(t)))
            .unwrap_or((false, false));
        return (
            Vec::new(),
            vec![Cmd {
                part: 0,
                text,
                test,
                commit,
                codemode: false,
            }],
        );
    }
    if tool == "exec" {
        let Some(v) = input.and_then(|i| serde_json::from_str::<Value>(i).ok()) else {
            return (Vec::new(), Vec::new());
        };
        let program = v.get("input").and_then(Value::as_str).unwrap_or("");
        let mut files = Vec::new();
        for call in codemode::apply_patches(program) {
            if let Some(p) = call.value {
                files.extend(
                    edits::paths_of_patch(&p)
                        .iter()
                        .map(|f| display_path(f, roots)),
                );
            }
        }
        let cmds = codemode::exec_commands(program)
            .into_iter()
            .enumerate()
            .map(|(part, c)| {
                let (test, commit) = c
                    .value
                    .as_deref()
                    .map(|t| (commands::is_test(t), commands::is_commit(t)))
                    .unwrap_or((false, false));
                Cmd {
                    part,
                    text: c.value,
                    test,
                    commit,
                    codemode: true,
                }
            })
            .collect();
        return (files, cmds);
    }
    (Vec::new(), Vec::new())
}

/// A path as served: relative to the project root (or a member's root) when
/// inside it, else unchanged.
pub fn display_path(path: &str, roots: &[String]) -> String {
    for root in roots {
        let root = root.trim_end_matches('/');
        if let Some(rest) = path.strip_prefix(root) {
            if let Some(rel) = rest.strip_prefix('/') {
                return rel.to_string();
            }
        }
    }
    path.to_string()
}

/// How one command ended (see [`commands`]).
pub(crate) fn outcome_of(
    rows: &[Row],
    row: &Row,
    cmd: Option<&Cmd>,
    result_text: &HashMap<usize, String>,
) -> Outcome {
    let Some(ri) = row.result else {
        return Outcome::Unknown;
    };
    if cmd.is_some_and(|c| c.codemode) {
        // One program, one result: only an exit code the output states for a
        // program that ran exactly this one command speaks for it.
        if row.cmds.len() != 1 {
            return Outcome::Unknown;
        }
        let text = result_text.get(&ri).map(String::as_str).unwrap_or("");
        return match commands::exit_code(text) {
            Some(0) => Outcome::Succeeded,
            Some(_) => Outcome::Failed,
            // No exit code stated: a test runner's own summary of this one
            // command is the next-best record of how it ended.
            None => match cmd.filter(|c| c.test).and_then(|_| tally::tally(text)) {
                Some(t) if t.failed > 0 || t.errors > 0 => Outcome::Failed,
                Some(t) if t.passed > 0 => Outcome::Succeeded,
                _ => Outcome::Unknown,
            },
        };
    }
    if rows[ri].is_error {
        Outcome::Failed
    } else {
        Outcome::Succeeded
    }
}

/// Parse an RFC 3339 instant to milliseconds since the epoch.
pub fn parse_ms(ts: &str) -> Option<i64> {
    let b = ts.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') {
        return None;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let y = num(&ts[0..4])?;
    let mo = num(&ts[5..7])?;
    let d = num(&ts[8..10])?;
    let h = num(&ts[11..13])?;
    let mi = num(&ts[14..16])?;
    let s = num(&ts[17..19])?;
    let mut rest = &ts[19..];
    let mut ms = 0i64;
    if let Some(frac) = rest.strip_prefix('.') {
        let digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
        let padded: String = digits.chars().chain("000".chars()).take(3).collect();
        ms = num(&padded)?;
        rest = &frac[digits.len()..];
    }
    let offset_min = match rest {
        "" | "Z" | "z" => 0,
        o if o.len() == 6 && (o.starts_with('+') || o.starts_with('-')) => {
            let sign = if o.starts_with('-') { -1 } else { 1 };
            sign * (num(&o[1..3])? * 60 + num(&o[4..6])?)
        }
        _ => return None,
    };
    // Days from civil (Howard Hinnant).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + h * 3600 + mi * 60 + s - offset_min * 60) * 1000) + ms)
}

/// Mask `raw` and fold what the masker did into `record`.
pub(crate) fn masked(raw: &str, record: &mut MaskRecord) -> String {
    let (text, rec) = crate::redact::mask_recorded(raw);
    record.merge(&rec);
    text
}

/* ───────────────────────────── summaries ───────────────────────────── */

impl Index {
    pub fn project(&self) -> &HqProject {
        &self.project
    }

    /// Every row the index holds, by event id, in the order it holds them.
    /// For tests that compare two ways of building one index.
    #[doc(hidden)]
    pub fn row_ids(&self) -> Vec<&str> {
        self.rows.iter().map(|r| r.id.as_str()).collect()
    }

    pub(crate) fn order_key(&self, i: usize) -> (i64, i64) {
        (self.rows[i].ms, self.rows[i].seq)
    }

    pub(crate) fn outcome(&self, row: usize, cmd: &Cmd) -> Outcome {
        outcome_of(&self.rows, &self.rows[row], Some(cmd), &self.result_text)
    }

    /// The edit count of a row: one per file.
    fn edit_count(&self, i: usize) -> u64 {
        self.rows[i].files.len() as u64
    }

    pub(crate) fn summary(
        &self,
        t: &Thread,
        now_ms: i64,
        record: &mut MaskRecord,
    ) -> ThreadSummary {
        let rows = &self.rows;
        let (title, title_basis, title_row) = self.title_of(t, record);
        let mut files: BTreeMap<String, u64> = BTreeMap::new();
        let mut commands = Counts::default();
        let mut tests = Counts::default();
        let mut commits = 0u64;
        let mut claims = 0u64;
        let mut edits = 0u64;
        let mut last_test: Option<(usize, &Cmd, Outcome)> = None;
        let mut last_pass: Option<usize> = None;
        for &i in &t.rows {
            let r = &rows[i];
            for f in &r.files {
                *files.entry(f.clone()).or_default() += 1;
            }
            edits += self.edit_count(i);
            for c in &r.cmds {
                let o = self.outcome(i, c);
                commands.add(o);
                if c.test {
                    tests.add(o);
                    last_test = Some((i, c, o));
                    if o == Outcome::Succeeded {
                        last_pass = Some(i);
                    }
                }
                if c.commit && o != Outcome::Failed {
                    commits += 1;
                }
            }
            claims += self.claims.get(&i).map_or(0, |c| c.len() as u64);
        }
        let pass_pos = last_pass.and_then(|p| t.rows.iter().position(|&i| i == p));
        // A claim of finished work is open unless a passing test run backs
        // it: the latest test run before it passed and nothing was edited
        // between that run and the claim, or a passing test run follows it.
        let mut claim_open = false;
        {
            let mut latest_test: Option<Outcome> = None;
            let mut edited_since_test = false;
            let mut open_since_pass = false;
            let mut backed_claim_since_pass = false;
            for &i in &t.rows {
                let r = &rows[i];
                if !r.files.is_empty() {
                    edited_since_test = true;
                    if backed_claim_since_pass {
                        open_since_pass = true;
                    }
                }
                for c in r.cmds.iter().filter(|c| c.test) {
                    let o = self.outcome(i, c);
                    latest_test = Some(o);
                    edited_since_test = false;
                    if o == Outcome::Succeeded {
                        open_since_pass = false;
                        backed_claim_since_pass = false;
                    }
                }
                if self.claims.contains_key(&i) {
                    let backed = latest_test == Some(Outcome::Succeeded) && !edited_since_test;
                    if !backed {
                        open_since_pass = true;
                    } else {
                        backed_claim_since_pass = true;
                    }
                }
            }
            claim_open |= open_since_pass;
        }
        let mut edited_after: BTreeSet<String> = BTreeSet::new();
        if let Some(p) = pass_pos {
            for &i in &t.rows[p + 1..] {
                edited_after.extend(rows[i].files.iter().cloned());
            }
        }
        // Whose move.
        let recent = self
            .last_activity_ms(t)
            .is_some_and(|ms| is_recent(ms, now_ms));
        let last_words = t
            .rows
            .iter()
            .rev()
            .find(|&&i| rows[i].kind == Kind::Assistant)
            .and_then(|&i| rows[i].text.as_deref());
        let asked_tool = t
            .rows
            .iter()
            .rev()
            .find(|&&i| rows[i].kind == Kind::ToolUse)
            .and_then(|&i| rows[i].tool_name.as_deref())
            .is_some_and(|n| matches!(n, "AskUserQuestion" | "request_user_input_async"));
        let asked = asked_tool || last_words.is_some_and(|w| w.trim_end().ends_with('?'));
        let (whose_move, basis) = if recent {
            (WhoseMove::Agent, "recent_activity")
        } else if last_test
            .as_ref()
            .is_some_and(|(_, _, o)| *o == Outcome::Failed)
        {
            (WhoseMove::You, "last_test_failed")
        } else if claim_open {
            (WhoseMove::You, "claim_without_passing_test")
        } else if asked {
            (WhoseMove::You, "agent_asked")
        } else {
            (WhoseMove::Done, "nothing_open")
        };
        let waiting_event_id = if basis == "agent_asked" {
            let last_user = t.rows.iter().rposition(|&i| rows[i].kind == Kind::User);
            t.rows.iter().enumerate().rev().find_map(|(pos, &i)| {
                if last_user.is_some_and(|user| pos <= user) {
                    return None;
                }
                let row = &rows[i];
                let question = (row.kind == Kind::ToolUse
                    && matches!(
                        row.tool_name.as_deref(),
                        Some("AskUserQuestion" | "request_user_input_async")
                    ))
                    || (row.kind == Kind::Assistant
                        && row
                            .text
                            .as_deref()
                            .is_some_and(|text| text.trim_end().ends_with('?')));
                question.then(|| row.id.clone())
            })
        } else {
            None
        };
        let question_pos = waiting_event_id
            .as_ref()
            .and_then(|id| t.rows.iter().position(|&i| rows[i].id == *id));
        let mut step = 0u64;
        let mut waiting_command = None;
        let mut latest_claim = None;
        let mut file_removals = Vec::new();
        let mut trace: BTreeMap<usize, Vec<ThreadTraceEvent>> = BTreeMap::new();
        for (pos, &i) in t.rows.iter().enumerate() {
            let row = &rows[i];
            let events = trace.entry(row.version).or_default();
            if row.prompt.is_some() {
                step += 1;
                events.push(ThreadTraceEvent {
                    kind: "prompt",
                    step,
                    file: None,
                    result: None,
                });
            }
            for file in &row.files {
                step += 1;
                events.push(ThreadTraceEvent {
                    kind: "edit",
                    step,
                    file: Some(masked(file, record)),
                    result: None,
                });
            }
            for command in &row.cmds {
                step += 1;
                let outcome = self.outcome(i, command);
                events.push(ThreadTraceEvent {
                    kind: if command.test { "run" } else { "command" },
                    step,
                    file: None,
                    result: if command.test {
                        match outcome {
                            Outcome::Succeeded => Some("passed"),
                            Outcome::Failed => Some("failed"),
                            Outcome::Unknown => None,
                        }
                    } else if row.result.is_some() {
                        Some("ran")
                    } else {
                        None
                    },
                });
                if question_pos.is_some_and(|asked| pos > asked)
                    && row.result.is_none()
                    && outcome == Outcome::Unknown
                {
                    if let Some(raw) = command.text.as_deref().filter(|text| !text.is_empty()) {
                        waiting_command = Some(WaitingCommand {
                            command: first_line(raw, 200, record),
                            step,
                            at: row.ts.clone(),
                        });
                    }
                }
                if row.result.is_some() {
                    if let Some(raw) = command.text.as_deref() {
                        let removal = raw.split("&&").any(|part| {
                            let mut words = part.split_whitespace();
                            words.next() == Some("rm")
                                && words.next().is_some_and(|flags| {
                                    flags.starts_with('-')
                                        && flags.contains('r')
                                        && flags.contains('f')
                                })
                        });
                        if removal {
                            file_removals.push(FileRemoval {
                                command: first_line(raw, 200, record),
                                at: row.ts.clone(),
                            });
                        }
                    }
                }
            }
            if let Some(claims) = self.claims.get(&i) {
                for claim in claims {
                    step += 1;
                    events.push(ThreadTraceEvent {
                        kind: "claim",
                        step,
                        file: None,
                        result: None,
                    });
                    let agent = match self.sessions[row.session].source.as_str() {
                        "claude_code" => "Claude Code",
                        "codex" => "Codex",
                        _ => "the agent",
                    };
                    latest_claim = Some(LatestClaim {
                        agent: agent.to_string(),
                        text: first_line(&claim.text, 200, record),
                        at: row.ts.clone(),
                        step,
                    });
                }
            }
        }
        let mut top: Vec<FileCount> = files
            .iter()
            .map(|(p, n)| FileCount {
                path: masked(p, record),
                edits: *n,
            })
            .collect();
        top.sort_by(|a, b| b.edits.cmp(&a.edits).then_with(|| a.path.cmp(&b.path)));
        top.truncate(5);
        let first = t.rows.first().copied();
        let last_any = t.rows.last().copied();
        let current_version = last_any.map(|i| rows[i].version);
        // The drawing has room for three version bands and five steps in each.
        // Keep only the newest recorded slice, with no invented intermediate work.
        let version_thread = if matches!(basis, "claim_without_passing_test" | "last_test_failed") {
            let mut newest: Vec<_> = trace.into_iter().rev().take(3).collect();
            newest.reverse();
            newest
                .into_iter()
                .filter_map(|(version, events)| {
                    self.versions.get(version).map(|v| {
                        let ran = events.iter().any(|e| e.kind == "run");
                        let mut recent: Vec<_> = events.into_iter().rev().take(5).collect();
                        recent.reverse();
                        ThreadVersionTrace {
                            version: v.id.clone(),
                            now: current_version == Some(version),
                            ran,
                            events: recent,
                        }
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        ThreadSummary {
            id: t.id.clone(),
            title,
            title_basis,
            title_event_id: rows[title_row].id.clone(),
            started_at: first.and_then(|i| rows[i].ts.clone()),
            last_activity_at: last_any.and_then(|i| rows[i].ts.clone()),
            sessions: t.sessions.len() as u64,
            prompts: t.work_prompts.len() as u64,
            conversation: t.conversation.len() as u64,
            edits,
            files: files.len() as u64,
            top_files: top,
            commands,
            tests,
            commits,
            claims,
            whose_move,
            whose_move_basis: basis,
            waiting_event_id,
            waiting_command,
            latest_claim,
            version_thread,
            file_removals,
            proof: Proof {
                last_test: last_test.map(|(i, c, o)| TestRef {
                    event_id: rows[i].id.clone(),
                    command_id: command_id(&rows[i].id, c),
                    at: rows[i].ts.clone(),
                    outcome: o,
                    command: first_line(c.text.as_deref().unwrap_or(""), 200, record),
                }),
                last_passing_test_at: last_pass.and_then(|i| rows[i].ts.clone()),
                edited_after_last_pass: edited_after.iter().map(|f| masked(f, record)).collect(),
            },
            current_version_id: last_any
                .and_then(|i| self.versions.get(rows[i].version))
                .map(|v| v.id.clone()),
            told_agent: None,
        }
    }

    /// The title rule's answer for one thread: its prompts in order, the
    /// first work prompt first; the title rule ([`prompts::thread_title`])
    /// picks the sentence and the prompt it came from, which
    /// `title_event_id` names. The title is masked and cut, and its masking
    /// is folded into `record`. Shared by [`Index::summary`] and the record
    /// search ([`search`]), so a Find row names its Change exactly as the
    /// Inbox does.
    pub(crate) fn title_of(
        &self,
        t: &Thread,
        record: &mut MaskRecord,
    ) -> (String, prompts::TitleBasis, usize) {
        let rows = &self.rows;
        let mut order: Vec<usize> = vec![t.first_prompt];
        let mut later: Vec<usize> = t
            .work_prompts
            .iter()
            .chain(t.conversation.iter())
            .copied()
            .filter(|&i| i != t.first_prompt)
            .collect();
        later.sort_by_key(|&i| self.order_key(i));
        order.extend(later);
        let texts: Vec<&str> = order
            .iter()
            .map(|&i| rows[i].prompt.as_deref().unwrap_or(""))
            .collect();
        let (title_raw, title_basis, at) = prompts::thread_title(&texts);
        let title_row = order.get(at).copied().unwrap_or(t.first_prompt);
        let title = title_raw
            .map(|s| {
                let (title, rec) = prompts::cut_title(&s);
                record.merge(&rec);
                title
            })
            .unwrap_or_default();
        (title, title_basis, title_row)
    }

    /// C27: the one thing in a thread's view that depends on the clock —
    /// whether its last activity is within [`AGENT_ACTIVE_MS`] of `now_ms`.
    /// Two `thread_view`s of one index at two instants with the same answer
    /// here are identical, so a server may cache the serialized view keyed on
    /// it. `None` when the project has no such thread.
    pub fn thread_active_at(&self, thread_id: &str, now_ms: i64) -> Option<bool> {
        let t = self.threads.iter().find(|t| t.id == thread_id)?;
        Some(
            self.last_activity_ms(t)
                .is_some_and(|ms| is_recent(ms, now_ms)),
        )
    }

    /// C27 (c): the one thing in the project view ([`Index::view`]) that
    /// depends on the clock — the ids of the threads whose last activity is
    /// within [`AGENT_ACTIVE_MS`] of `now_ms`, in the index's thread order.
    /// The clock reaches the view only through each thread's `recent` flag
    /// in `summary` (its `whose_move`, and from it the project's
    /// `whose_move` counts); thread order, titles and every count are fixed
    /// by the rows. So two views of one index at two instants with the same
    /// answer here are identical, and a server may cache the serialized view
    /// keyed on it. Pinned by `tests/hq_project_cache_key.rs`.
    pub fn active_threads_at(&self, now_ms: i64) -> Vec<&str> {
        self.threads
            .iter()
            .filter(|t| {
                self.last_activity_ms(t)
                    .is_some_and(|ms| is_recent(ms, now_ms))
            })
            .map(|t| t.id.as_str())
            .collect()
    }

    /// The instant of a thread's last activity row (anything but an injected
    /// user row) — what "the agent is working" is judged on.
    fn last_activity_ms(&self, t: &Thread) -> Option<i64> {
        t.rows
            .iter()
            .rev()
            .copied()
            .find(|&i| self.rows[i].kind != Kind::User || self.rows[i].prompt.is_some())
            .map(|i| self.rows[i].ms)
    }

    /// The project's list row, kept so it can be served at any later `now`
    /// without the index: everything in it is fixed by the rows except
    /// `whose_move`, whose `agent` count depends on the clock, so each
    /// thread's last-activity instant and its verdict when the agent is not
    /// active are kept beside it.
    pub fn listing(&self) -> Listing {
        let mut record = MaskRecord::default();
        let threads: Vec<ThreadSummary> = self
            .threads
            .iter()
            .map(|t| self.summary(t, i64::MIN, &mut record))
            .collect();
        let moves = self
            .threads
            .iter()
            .zip(&threads)
            .map(|(t, s)| (self.last_activity_ms(t), s.whose_move))
            .collect();
        Listing {
            summary: self.project_summary(&threads, &mut record),
            moves,
        }
    }

    /// The project view at `now_ms`, with no told-the-agent acts.
    pub fn view(&self, now_ms: i64) -> ProjectView {
        self.view_told(now_ms, &BTreeMap::new())
    }

    /// The project view at `now_ms`, with each thread's last told-the-agent act
    /// (C30 R-LEDGER) served as `told_agent` beside it. `told` is keyed by
    /// thread id; a thread with no entry serves no `told_agent`, so an empty
    /// map is byte-for-byte [`Index::view`]. The acts reach the view only
    /// through that one field: they move no `whose_move`, no count and no
    /// order. A server that caches the serialized view must key it on the act
    /// ids it passes here as well as on the index.
    pub fn view_told(
        &self,
        now_ms: i64,
        told: &BTreeMap<String, crate::change_agent_message::ToldAct>,
    ) -> ProjectView {
        let mut record = MaskRecord::default();
        let mut keyed: Vec<(i64, ThreadSummary)> = self
            .threads
            .iter()
            .map(|t| {
                let last = t.rows.last().map_or(0, |&i| self.rows[i].ms);
                let mut summary = self.summary(t, now_ms, &mut record);
                summary.told_agent = told.get(&t.id).map(|act| self.served_told(t, act));
                (last, summary)
            })
            .collect();
        keyed.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        let threads: Vec<ThreadSummary> = keyed.into_iter().map(|(_, t)| t).collect();
        let project = self.project_summary(&threads, &mut record);
        ProjectView {
            schema_version: SCHEMA_VERSION,
            project,
            threads,
            masking: record,
        }
    }

    /// One thread's last told-the-agent act, served: `later_work` is whether
    /// any row of the thread was recorded after the act. Derived at read, never
    /// stored.
    fn served_told(
        &self,
        t: &Thread,
        act: &crate::change_agent_message::ToldAct,
    ) -> crate::change_agent_message::ToldAgent {
        act.served(self.has_row_after(t, &act.at))
    }

    /// [`ThreadSummary::told_agent`] for one thread of this project, or `None`
    /// when the project has no such thread.
    pub fn told_agent(
        &self,
        thread_id: &str,
        act: &crate::change_agent_message::ToldAct,
    ) -> Option<crate::change_agent_message::ToldAgent> {
        let t = self.threads.iter().find(|t| t.id == thread_id)?;
        Some(self.served_told(t, act))
    }

    /// Does the thread hold a row recorded after the instant `at` names? An
    /// act is stamped to the second, so a row counts only from the NEXT whole
    /// second: a row logged in the same second as the act, but before it, must
    /// not read as work the act came before. An unreadable stamp is "no".
    fn has_row_after(&self, t: &Thread, at: &str) -> bool {
        let Some(at_ms) = parse_ms(at) else {
            return false;
        };
        let floor = at_ms.div_euclid(1000) * 1000 + 1000;
        t.rows.iter().any(|&i| self.rows[i].ms >= floor)
    }

    fn project_summary(
        &self,
        threads: &[ThreadSummary],
        record: &mut MaskRecord,
    ) -> ProjectSummary {
        let mut commands = Counts::default();
        let mut tests = Counts::default();
        let mut edits = 0u64;
        let mut prompts = 0u64;
        let mut conversation = 0u64;
        let mut claims = 0u64;
        let in_thread: BTreeSet<usize> = self
            .threads
            .iter()
            .flat_map(|t| t.work_prompts.iter().copied())
            .collect();
        for (i, r) in self.rows.iter().enumerate() {
            if r.copied {
                continue;
            }
            if r.prompt.is_some() {
                if in_thread.contains(&i) {
                    prompts += 1;
                } else {
                    conversation += 1;
                }
            }
            edits += self.edit_count(i);
            for c in &r.cmds {
                let o = self.outcome(i, c);
                commands.add(o);
                if c.test {
                    tests.add(o);
                }
            }
            claims += self.claims.get(&i).map_or(0, |c| c.len() as u64);
        }
        let mut whose: BTreeMap<String, u64> = BTreeMap::new();
        for k in ["you", "agent", "done"] {
            whose.insert(k.to_string(), 0);
        }
        for t in threads {
            let k = match t.whose_move {
                WhoseMove::You => "you",
                WhoseMove::Agent => "agent",
                WhoseMove::Done => "done",
            };
            *whose.entry(k.to_string()).or_default() += 1;
        }
        let mut sources: Vec<String> = self
            .project
            .members
            .iter()
            .map(|m| m.source.clone())
            .collect();
        sources.sort();
        sources.dedup();
        let first = self
            .sessions
            .iter()
            .filter_map(|s| s.first_ts.clone())
            .min();
        let last = self.sessions.iter().filter_map(|s| s.last_ts.clone()).max();
        ProjectSummary {
            complete: true,
            id: self.project.id.clone(),
            name: masked(&self.project.name, record),
            root: masked(&self.project.shown_root(), record),
            path_known: self.project.path_known(),
            sources,
            members: self
                .project
                .members
                .iter()
                .map(|m| Member {
                    ledger_project_id: m.ledger_project_id.clone(),
                    source: m.source.clone(),
                    root: masked(&m.root, record),
                    root_basis: m.root_basis,
                })
                .collect(),
            sessions: self.sessions.len() as u64,
            prompts,
            conversation_prompts: conversation,
            edits,
            commands,
            tests,
            claims,
            first_activity: first,
            last_activity: last,
            threads: self.threads.len() as u64,
            conversation_only: self.conversation_only,
            unattached_turns: self.unattached_turns,
            copied_rows_skipped: self.copied,
            whose_move: whose,
        }
    }
}

/// Is a row stamped `ms` within [`AGENT_ACTIVE_MS`] of `now_ms` (either side)?
/// Saturating, so `now_ms = i64::MIN` means "never recent".
fn is_recent(ms: i64, now_ms: i64) -> bool {
    now_ms.saturating_sub(ms) <= AGENT_ACTIVE_MS && ms <= now_ms.saturating_add(AGENT_ACTIVE_MS)
}

/// A project's list row as the index computed it, servable at any `now`
/// ([`Index::listing`]).
#[derive(Debug, Clone)]
pub struct Listing {
    summary: ProjectSummary,
    /// Per thread: last-activity instant, and its move when the agent is not
    /// working.
    moves: Vec<(Option<i64>, WhoseMove)>,
}

impl Listing {
    pub fn at(&self, now_ms: i64) -> ProjectSummary {
        let mut summary = self.summary.clone();
        let mut whose: BTreeMap<String, u64> = ["you", "agent", "done"]
            .iter()
            .map(|k| (k.to_string(), 0))
            .collect();
        for (ms, idle) in &self.moves {
            let k = if ms.is_some_and(|ms| is_recent(ms, now_ms)) {
                "agent"
            } else {
                match idle {
                    WhoseMove::You => "you",
                    WhoseMove::Agent => "agent",
                    WhoseMove::Done => "done",
                }
            };
            *whose.entry(k.to_string()).or_default() += 1;
        }
        summary.whose_move = whose;
        summary
    }
}

/// A project's list row before its index exists: only what the database
/// answers without reading the project's rows — membership, recordings,
/// row count and first/last instant (one probe of `idx_events_project_ts`
/// each). Every field the index decides is absent (`null`), never 0.
#[derive(Debug, Clone, Serialize)]
pub struct QuickSummary {
    pub id: String,
    pub name: String,
    pub root: String,
    /// See [`ProjectSummary::path_known`].
    pub path_known: bool,
    pub sources: Vec<String>,
    pub members: Vec<Member>,
    pub sessions: u64,
    /// Recorded rows of every kind (the members' chain positions).
    pub events: u64,
    pub first_activity: Option<String>,
    pub last_activity: Option<String>,
    /// Always `false`: the counts below are not known yet.
    pub complete: bool,
    pub prompts: Option<u64>,
    pub conversation_prompts: Option<u64>,
    pub edits: Option<u64>,
    pub commands: Option<Counts>,
    pub tests: Option<Counts>,
    pub claims: Option<u64>,
    pub threads: Option<u64>,
    pub conversation_only: Option<u64>,
    pub unattached_turns: Option<u64>,
    pub copied_rows_skipped: Option<u64>,
    pub whose_move: Option<BTreeMap<String, u64>>,
}

/// What tracking one thread as a Change needs to know about it.
#[derive(Debug, Clone)]
pub struct ThreadIdentity {
    pub id: String,
    /// The served title (C26 rule, masked, cut) — the same text the screens
    /// show.
    pub title: String,
    /// The event the title came from.
    pub title_event_id: String,
    /// The ledger project of the thread's first work prompt.
    pub ledger_project_id: String,
    /// The first work prompt's event.
    pub first_prompt_event_id: String,
    /// The version the thread is at: the one its last row belongs to.
    pub current_version_id: Option<String>,
    /// Every session of the thread, with its ledger project, in session order.
    pub sessions: Vec<(String, String)>,
    pub masking: MaskRecord,
}

impl Index {
    /// [`ThreadIdentity`] for one thread of this project, or `None`.
    pub fn thread_identity(&self, thread_id: &str) -> Option<ThreadIdentity> {
        let t = self.threads.iter().find(|t| t.id == thread_id)?;
        let mut record = MaskRecord::default();
        let summary = self.summary(t, i64::MIN, &mut record);
        let first = &self.rows[t.first_prompt];
        let mut sessions: Vec<(String, String)> = t
            .sessions
            .iter()
            .map(|&s| {
                let info = &self.sessions[s];
                (info.id.clone(), info.ledger_project.clone())
            })
            .collect();
        sessions.sort();
        Some(ThreadIdentity {
            id: t.id.clone(),
            title: summary.title,
            title_event_id: summary.title_event_id,
            ledger_project_id: self.sessions[first.session].ledger_project.clone(),
            first_prompt_event_id: first.id.clone(),
            current_version_id: summary.current_version_id,
            sessions,
            masking: record,
        })
    }

    /// A current Inbox row's content address. The first work prompt is the
    /// stable FK anchor; every event in thread order enters the digest, so a
    /// newly recorded step makes an earlier hide cease to match. Prompt is
    /// offered only while the thread actually files as an agent question.
    pub fn inbox_address(
        &self,
        thread_id: &str,
        kind: crate::inbox_hide::RowKind,
        now_ms: i64,
    ) -> Option<crate::inbox_hide::Address> {
        let t = self.threads.iter().find(|t| t.id == thread_id)?;
        let mut record = MaskRecord::default();
        let summary = self.summary(t, now_ms, &mut record);
        let current_kind =
            if summary.whose_move == WhoseMove::You && summary.whose_move_basis == "agent_asked" {
                crate::inbox_hide::RowKind::Prompt
            } else {
                crate::inbox_hide::RowKind::Thread
            };
        if kind != current_kind {
            return None;
        }
        Some(self.inbox_address_for_thread(t, kind))
    }

    fn inbox_address_for_thread(
        &self,
        t: &Thread,
        kind: crate::inbox_hide::RowKind,
    ) -> crate::inbox_hide::Address {
        crate::inbox_hide::Address {
            row_kind: kind,
            thread_id: t.id.clone(),
            thread_event_id: self.rows[t.first_prompt].id.clone(),
            row_sha256: match kind {
                crate::inbox_hide::RowKind::Thread => t.inbox_thread_sha256.clone(),
                crate::inbox_hide::RowKind::Prompt => t.inbox_prompt_sha256.clone(),
            },
        }
    }

    /// The addresses the Inbox may act on, in project thread order. An asked
    /// thread has a prompt address; other threads have their one thread row.
    pub fn inbox_addresses(&self, now_ms: i64) -> Vec<crate::inbox_hide::Address> {
        self.threads
            .iter()
            .map(|t| {
                let mut record = MaskRecord::default();
                let summary = self.summary(t, now_ms, &mut record);
                let kind = if summary.whose_move == WhoseMove::You
                    && summary.whose_move_basis == "agent_asked"
                {
                    crate::inbox_hide::RowKind::Prompt
                } else {
                    crate::inbox_hide::RowKind::Thread
                };
                self.inbox_address_for_thread(t, kind)
            })
            .collect()
    }
}

/// Does any project HQ lists hold a recorded row? One index probe per
/// ledger project (`idx_events_project_ts`), never an index build — the
/// question `thruline ui` asks at start to choose between the Inbox and
/// First run (C27). A project with no rows (picked, nothing recorded yet)
/// and work recorded only in a temporary folder do not count.
pub fn has_records(ledger: &Ledger) -> Result<bool> {
    for project in hq_projects(ledger)? {
        for m in &project.members {
            let any: bool = ledger.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE project_id = ?1)",
                [&m.ledger_project_id],
                |r| r.get(0),
            )?;
            if any {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// [`QuickSummary`] for one project.
pub fn quick_summary(ledger: &Ledger, project: &HqProject) -> Result<QuickSummary> {
    let conn = ledger.conn();
    let mut sessions = 0u64;
    let mut events = 0u64;
    let mut first: Option<String> = None;
    let mut last: Option<String> = None;
    for m in &project.members {
        let id = &m.ledger_project_id;
        sessions += conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE project_id = ?1",
            [id],
            |r| r.get::<_, i64>(0),
        )? as u64;
        events += conn
            .query_row(
                "SELECT event_total FROM project_heads WHERE project_id = ?1",
                [id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64;
        // Two index probes; `ts IS NOT NULL` keeps unstamped rows out of MIN.
        let lo: Option<String> = conn.query_row(
            "SELECT MIN(ts) FROM events WHERE project_id = ?1 AND ts IS NOT NULL",
            [id],
            |r| r.get(0),
        )?;
        let hi: Option<String> = conn.query_row(
            "SELECT MAX(ts) FROM events WHERE project_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        first = match (first, lo) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        last = last.max(hi);
    }
    let mut record = MaskRecord::default();
    let mut sources: Vec<String> = project.members.iter().map(|m| m.source.clone()).collect();
    sources.sort();
    sources.dedup();
    Ok(QuickSummary {
        id: project.id.clone(),
        name: masked(&project.name, &mut record),
        root: masked(&project.shown_root(), &mut record),
        path_known: project.path_known(),
        sources,
        members: project
            .members
            .iter()
            .map(|m| Member {
                ledger_project_id: m.ledger_project_id.clone(),
                source: m.source.clone(),
                root: masked(&m.root, &mut record),
                root_basis: m.root_basis,
            })
            .collect(),
        sessions,
        events,
        first_activity: first,
        last_activity: last,
        complete: false,
        prompts: None,
        conversation_prompts: None,
        edits: None,
        commands: None,
        tests: None,
        claims: None,
        threads: None,
        conversation_only: None,
        unattached_turns: None,
        copied_rows_skipped: None,
        whose_move: None,
    })
}

pub(crate) fn command_id(event_id: &str, c: &Cmd) -> String {
    if c.codemode {
        format!("{event_id}#{}", c.part)
    } else {
        event_id.to_string()
    }
}

/// The first line of `raw`, masked first and then cut to `max` characters
/// with `…` (never inside a `[masked: …]` marker).
pub(crate) fn first_line(raw: &str, max: usize, record: &mut MaskRecord) -> String {
    let (text, rec) = crate::redact::mask_recorded(raw);
    record.merge(&rec);
    let line = text.lines().next().unwrap_or("");
    if line.chars().count() <= max && text.lines().nth(1).is_none() {
        return line.to_string();
    }
    let mut cut = crate::redact::truncate_respecting_markers(line, max.saturating_sub(1));
    cut.push('…');
    cut
}

/// Every HQ project's summary (each project's index is built once).
pub fn projects(ledger: &Ledger, now_ms: i64) -> Result<Vec<ProjectSummary>> {
    let mut out = Vec::new();
    for p in hq_projects(ledger)? {
        out.push(index(ledger, &p)?.view(now_ms).project);
    }
    Ok(out)
}

/// One HQ project's view, or `None` when no project has this id.
pub fn project(ledger: &Ledger, project_id: &str, now_ms: i64) -> Result<Option<ProjectView>> {
    match find_project(ledger, project_id)? {
        Some(p) => Ok(Some(index(ledger, &p)?.view(now_ms))),
        None => Ok(None),
    }
}

/* ───────────────────────────── loading ───────────────────────────── */

/// The recordings on this machine that belong to a root.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LoadPlan {
    pub claude_code: Vec<PathBuf>,
    pub codex: Vec<PathBuf>,
    pub bytes: u64,
    /// C34 pause gate: recordings left out of this plan because their folder
    /// is paused now. Zero until [`load::apply_pause_gate`] has run.
    pub paused_by_you: u64,
    /// C34 pause gate: recordings left out because their session started
    /// inside a pause.
    pub started_while_paused: u64,
}

/// Find every recording under `claude_projects` and `codex_roots` that
/// belongs to `root`:
///
/// * Claude Code: each folder whose name is the vendor's mangling of `root`
///   ([`crate::speaker::mangle_root`]) or begins with it followed by `-`
///   (worktrees under `.claude/worktrees/`, subdirectories), whose top-level
///   transcripts state a `cwd` inside `root`. A folder whose name is exactly
///   the mangled root and states no `cwd` at all is included on the name.
///   Only top-level `*.jsonl` files are taken — the same files `ingest` reads.
/// * Codex: every `*.jsonl` under the roots (recursively, symlinks not
///   followed) whose first line's `session_meta` `cwd` is inside `root`.
pub fn load_plan(root: &str, claude_projects: &Path, codex_roots: &[PathBuf]) -> LoadPlan {
    let mut plan = LoadPlan::default();
    let mangled = crate::speaker::mangle_root(root);
    if let Ok(entries) = std::fs::read_dir(claude_projects) {
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let exact = name == mangled;
            if !exact && !name.starts_with(&format!("{mangled}-")) {
                continue;
            }
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map(|it| {
                    it.filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "jsonl"))
                        .collect()
                })
                .unwrap_or_default();
            files.sort();
            // The first recorded `cwd` inside `root` decides; only a folder
            // whose transcripts state no `cwd` at all falls back to its name.
            // (C27: stops at that first file instead of reading every one.)
            let mut any_cwd = false;
            let belongs = files.iter().filter_map(|f| recorded_cwd(f)).any(|c| {
                any_cwd = true;
                is_under(&worktree_parent(&c), root) || is_under(&c, root)
            }) || (!any_cwd && exact);
            if belongs {
                plan.claude_code.extend(files);
            }
        }
    }
    for base in codex_roots {
        let mut stack = vec![base.clone()];
        let mut found = Vec::new();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.filter_map(|e| e.ok()) {
                let Ok(ft) = e.file_type() else { continue };
                let p = e.path();
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    stack.push(p);
                } else if ft.is_file() && p.extension().is_some_and(|x| x == "jsonl") {
                    found.push(p);
                }
            }
        }
        found.sort();
        for f in found {
            if recorded_cwd(&f).is_some_and(|c| is_under(&c, root)) {
                plan.codex.push(f);
            }
        }
    }
    plan.bytes = plan
        .claude_code
        .iter()
        .chain(plan.codex.iter())
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reader_refuses_a_held_scratch_store_before_its_read_transaction() {
        let root = tempfile::tempdir().expect("scratch directory");
        let db = root.path().join("ledger.db");
        drop(Connection::open(&db).expect("scratch SQLite file"));
        let project = HqProject {
            id: "scratch".to_owned(),
            name: "scratch".to_owned(),
            root: root.path().display().to_string(),
            members: Vec::new(),
        };
        let path = db.to_str().expect("UTF-8 scratch path");
        let bytes = std::fs::read(&db).expect("scratch bytes");
        let (reader, _) = snapshot_reader(path, &project).expect("no hold read");
        assert!(reader.is_readonly(rusqlite::MAIN_DB).expect("mode"));
        drop(reader);

        let hold = crate::store_hold::StoreHold::create(&db, "removal-01", "P1")
            .expect("hold scratch store");
        assert!(matches!(
            snapshot_reader(path, &project),
            Err(crate::error::CoreError::StoreHeld { .. })
        ));
        hold.release().expect("release hold");
        assert!(snapshot_reader(path, &project).is_ok());
        assert_eq!(std::fs::read(&db).expect("scratch bytes"), bytes);
    }

    #[test]
    fn a_mangled_folder_name_decodes_against_a_root_the_record_states() {
        let known = vec!["/Users/u/Desktop/Telescope_project".to_string()];
        assert_eq!(
            decode_folder_name(
                "-Users-u-Desktop-Telescope-project--claude-worktrees-competent-johnson-facb61",
                &known
            ),
            (
                "/Users/u/Desktop/Telescope_project/.claude/worktrees/competent-johnson-facb61"
                    .to_string(),
                "known_root"
            )
        );
        assert_eq!(
            decode_folder_name("-Users-u-Desktop-Telescope-project", &known),
            (
                "/Users/u/Desktop/Telescope_project".to_string(),
                "known_root"
            )
        );
    }

    #[test]
    fn a_mangled_folder_name_that_is_gone_decodes_dots_from_double_dashes() {
        assert_eq!(
            decode_folder_name(
                "-nowhere-hq-Desktop-Telescope-project--claude-worktrees-x",
                &[]
            ),
            (
                "/nowhere/hq/Desktop/Telescope/project/.claude/worktrees/x".to_string(),
                "folder_name"
            )
        );
    }

    #[test]
    fn instants_parse_to_epoch_milliseconds() {
        assert_eq!(parse_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ms("1970-01-01T00:00:01.5Z"), Some(1500));
        assert_eq!(parse_ms("2026-10-07T00:00:00Z"), Some(1_791_331_200_000));
        assert_eq!(
            parse_ms("2026-10-07T05:30:00+05:30"),
            Some(1_791_331_200_000)
        );
        assert_eq!(parse_ms("nope"), None);
    }

    #[test]
    fn paths_display_relative_to_the_root() {
        let roots = vec!["/r/app".to_string()];
        assert_eq!(display_path("/r/app/src/a.rs", &roots), "src/a.rs");
        assert_eq!(display_path("/r/app2/x", &roots), "/r/app2/x");
        assert_eq!(display_path("rel/x", &roots), "rel/x");
        assert!(is_under("/r/app/x", "/r/app"));
        assert!(!is_under("/r/app2", "/r/app"));
    }
}
