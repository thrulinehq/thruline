//! R04 — repo and worktree identity, and honest resolution of a project's root.
//!
//! **Additive by construction.** `project_id` is the munged transcript
//! directory name and the ledger's chain genesis derives from it
//! (`ledger::project_id_for`, `chain::genesis_prev_hash`), so repo identity
//! sits *alongside* project identity and never replaces it. Changing a
//! `project_id` would re-root a chain, which is the one thing the ledger exists
//! to prevent.
//!
//! **Abstention is a first-class outcome.** The A88 census measured repo
//! identity as underivable for 27 of 44 live projects, because the working
//! directory no longer exists — there is no root commit and no remote set to
//! hash. Those projects get [`Basis::Unresolvable`] and are *recorded* as
//! unknown. Guessing identity from the path is precisely the mistake the census
//! found: the stored `root_path` is wrong for 75% of the corpus.
//!
//! **Hashes are SHA-256** (A86 struck the pack's BLAKE3), lowercase hex, per
//! the standing convention.
//!
//! **Remote URLs are fingerprinted, never stored.** Identity needs to know that
//! two checkouts share a remote; it does not need to keep the URL, which can
//! carry a host, an org, and sometimes a username. The fingerprint is a hash of
//! the *normalised* URL, so `https://host/a/b.git` and `git@host:a/b` — the
//! same repo fetched two ways — produce the same value.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::capture::{git_optional, trusted_git};
use crate::chain::sha256_hex;
use crate::error::Result;
use crate::ledger::Ledger;
use rusqlite::{params, OptionalExtension};

/// Domain separators. Versioned so a future change to the derivation is a new
/// namespace rather than a silent re-identification of every repo on disk.
const REPO_DOMAIN: &str = "thruline.repo.v1";
const WORKTREE_DOMAIN: &str = "thruline.worktree.v1";
const MARKER_DOMAIN: &str = "thruline.marker.v1";

/// What an identity is founded on. Carried with the id so a consumer can never
/// mistake a weak basis for a strong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// Root commit(s) and remote fingerprints. Survives moving and re-cloning.
    Git,
    /// A `.thruline-id` marker in a non-git directory. Survives moving; does
    /// not survive copying, and cannot relate two clones.
    Marker,
    /// Nothing to derive from — the directory is gone, or is neither a git repo
    /// nor marked. Never guessed.
    Unresolvable,
}

impl Basis {
    pub fn as_str(self) -> &'static str {
        match self {
            Basis::Git => "git",
            Basis::Marker => "marker",
            Basis::Unresolvable => "unresolvable",
        }
    }
}

/// The identity of a repository, independent of where it currently sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoIdentity {
    /// `None` when the basis is [`Basis::Unresolvable`] — abstention, not a
    /// placeholder id.
    pub repo_id: Option<String>,
    pub basis: Basis,
    /// Sorted. Usually one; a repo with grafted history can have several.
    pub root_commits: Vec<String>,
    /// Sorted hashes of normalised remote URLs. The URLs themselves are not
    /// retained.
    pub remote_fingerprints: Vec<String>,
}

impl RepoIdentity {
    pub fn unresolvable() -> Self {
        RepoIdentity {
            repo_id: None,
            basis: Basis::Unresolvable,
            root_commits: Vec::new(),
            remote_fingerprints: Vec::new(),
        }
    }
}

/// One working tree of a repo. Worktrees of a repo share `repo_id` and differ
/// in `worktree_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeIdentity {
    pub worktree_id: String,
    pub repo_id: String,
    /// The worktree's own git dir, which is what makes it distinct.
    pub git_dir: String,
    pub path: String,
    pub branch: Option<String>,
    pub base_commit: Option<String>,
    /// True for a `git worktree add` tree, false for the primary checkout.
    pub is_linked: bool,
}

/// Normalise a remote URL so the same repo fetched over ssh and https agrees.
///
/// `git@github.com:Org/repo.git` and `https://github.com/Org/repo` both become
/// `github.com/Org/repo`. Case is preserved in the path because forges differ
/// on whether the path is case-sensitive; only the host is lowercased.
pub fn normalise_remote(url: &str) -> String {
    let s = url.trim();
    // Strip scheme.
    let s = match s.find("://") {
        Some(i) => &s[i + 3..],
        None => s,
    };
    // Strip credentials / ssh user.
    let s = match s.rfind('@') {
        Some(i) => &s[i + 1..],
        None => s,
    };
    // A colon after the host is one of two things. scp-style `host:path`
    // becomes `host/path`. A numeric `host:22` is a transport detail, not
    // identity, and is dropped — so `ssh://host:22/a/b` and `git@host:a/b`
    // agree, which is the whole point of normalising.
    let s = match s.find(':') {
        Some(i) => {
            let rest = &s[i + 1..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                format!("{}/{}", &s[..i], rest)
            } else {
                // A port must be followed by a path boundary or nothing;
                // `host:22abc` is not a port, so it stays an scp-style path.
                let after = &rest[digits.len()..];
                if after.is_empty() || after.starts_with('/') {
                    format!("{}{}", &s[..i], after)
                } else {
                    format!("{}/{}", &s[..i], rest)
                }
            }
        }
        None => s.to_string(),
    };
    let s = s.trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    let s = s.trim_end_matches('/');
    match s.find('/') {
        Some(i) => format!("{}{}", s[..i].to_lowercase(), &s[i..]),
        None => s.to_lowercase(),
    }
}

/// Fingerprint of a remote. The URL is not recoverable from it.
pub fn remote_fingerprint(url: &str) -> String {
    sha256_hex(normalise_remote(url).as_bytes())
}

/// Pure derivation: repo id from its two inputs. No I/O, so it is testable
/// without a filesystem and identical on every machine.
///
/// Inputs are sorted and length-prefixed, so `["ab","c"]` and `["a","bc"]`
/// cannot collide.
pub fn repo_id_from(root_commits: &[String], remote_fingerprints: &[String]) -> String {
    let roots: BTreeSet<&String> = root_commits.iter().collect();
    let remotes: BTreeSet<&String> = remote_fingerprints.iter().collect();
    let mut buf = String::from(REPO_DOMAIN);
    buf.push('\n');
    for r in roots {
        buf.push_str(&format!("{}:{}\n", r.len(), r));
    }
    buf.push('\n');
    for r in remotes {
        buf.push_str(&format!("{}:{}\n", r.len(), r));
    }
    sha256_hex(buf.as_bytes())
}

/// Pure derivation: worktree id from its repo and its own git dir.
pub fn worktree_id_from(repo_id: &str, git_dir: &str) -> String {
    let buf = format!(
        "{WORKTREE_DOMAIN}\n{}:{repo_id}\n{}:{git_dir}\n",
        repo_id.len(),
        git_dir.len()
    );
    sha256_hex(buf.as_bytes())
}

/// Pure derivation for the non-git fallback.
pub fn marker_repo_id(marker: &str) -> String {
    sha256_hex(format!("{MARKER_DOMAIN}\n{}:{marker}\n", marker.len()).as_bytes())
}

fn lines_of(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Derive repo identity for the repository containing `path`.
///
/// Returns [`Basis::Unresolvable`] rather than an error when the path is gone
/// or is not a repo: an absent directory is a fact about the corpus, not a
/// failure of the caller. A genuine error (no git binary) still surfaces.
pub fn repo_identity_at(path: &Path) -> Result<RepoIdentity> {
    if !path.is_dir() {
        return Ok(RepoIdentity::unresolvable());
    }
    let git = trusted_git()?;
    if git_optional(&git, path, &["rev-parse", "--show-toplevel"])?.is_none() {
        return Ok(marker_identity(path));
    }

    // `--all` so a repo with more than one root (grafted or merged histories)
    // is described by all of them rather than by whichever HEAD points at.
    let roots = git_optional(&git, path, &["rev-list", "--max-parents=0", "--all"])?
        .map(|s| lines_of(&s))
        .unwrap_or_default();

    let remotes = git_optional(&git, path, &["remote"])?
        .map(|s| lines_of(&s))
        .unwrap_or_default();
    let mut fingerprints: Vec<String> = Vec::new();
    for name in &remotes {
        if let Some(url) = git_optional(&git, path, &["remote", "get-url", name])? {
            let url = url.trim();
            if !url.is_empty() {
                fingerprints.push(remote_fingerprint(url));
            }
        }
    }
    fingerprints.sort();
    fingerprints.dedup();

    // A repo with no commits has no root commit. With no remote either, there
    // is nothing repo-specific to hash — abstain rather than mint an id that
    // every empty repo on the machine would share.
    if roots.is_empty() && fingerprints.is_empty() {
        return Ok(RepoIdentity::unresolvable());
    }

    let mut roots_sorted = roots.clone();
    roots_sorted.sort();
    roots_sorted.dedup();
    Ok(RepoIdentity {
        repo_id: Some(repo_id_from(&roots_sorted, &fingerprints)),
        basis: Basis::Git,
        root_commits: roots_sorted,
        remote_fingerprints: fingerprints,
    })
}

/// Non-git fallback: read an existing `.thruline-id` marker.
///
/// This function NEVER writes. Creating a marker puts a file in someone's
/// project, which is a consent decision (the same one M5 capture install makes
/// explicitly, with a plan and an apply step), not something identity
/// derivation may do as a side effect of being asked a question.
fn marker_identity(path: &Path) -> RepoIdentity {
    let marker = path.join(".thruline-id");
    let Ok(contents) = std::fs::read_to_string(&marker) else {
        return RepoIdentity::unresolvable();
    };
    let token = contents.trim();
    if token.is_empty() {
        return RepoIdentity::unresolvable();
    }
    RepoIdentity {
        repo_id: Some(marker_repo_id(token)),
        basis: Basis::Marker,
        root_commits: Vec::new(),
        remote_fingerprints: Vec::new(),
    }
}

/// Enumerate the worktrees of the repo containing `path`.
///
/// Empty when the path is not a resolvable repo — again a fact, not an error.
pub fn worktrees_at(path: &Path) -> Result<Vec<WorktreeIdentity>> {
    let identity = repo_identity_at(path)?;
    let Some(repo_id) = identity.repo_id else {
        return Ok(Vec::new());
    };
    if identity.basis != Basis::Git {
        return Ok(Vec::new());
    }
    let git = trusted_git()?;
    let Some(porcelain) = git_optional(&git, path, &["worktree", "list", "--porcelain"])? else {
        return Ok(Vec::new());
    };
    let common = git_optional(&git, path, &["rev-parse", "--git-common-dir"])?;

    let mut out = Vec::new();
    let mut cur_path: Option<String> = None;
    let mut cur_head: Option<String> = None;
    let mut cur_branch: Option<String> = None;

    // Records are separated by a blank line; the last one has no trailing
    // blank, so the flush is factored out.
    let mut flush = |p: &mut Option<String>, h: &mut Option<String>, b: &mut Option<String>| {
        if let Some(wt_path) = p.take() {
            let git_dir = worktree_git_dir(Path::new(&wt_path), common.as_deref());
            out.push(WorktreeIdentity {
                worktree_id: worktree_id_from(&repo_id, &git_dir),
                repo_id: repo_id.clone(),
                is_linked: Path::new(&wt_path).join(".git").is_file(),
                git_dir,
                path: wt_path,
                branch: b.take(),
                base_commit: h.take(),
            });
        } else {
            *h = None;
            *b = None;
        }
    };

    for line in porcelain.lines() {
        if line.trim().is_empty() {
            flush(&mut cur_path, &mut cur_head, &mut cur_branch);
        } else if let Some(rest) = line.strip_prefix("worktree ") {
            flush(&mut cur_path, &mut cur_head, &mut cur_branch);
            cur_path = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            cur_head = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("branch ") {
            cur_branch = Some(
                rest.trim()
                    .strip_prefix("refs/heads/")
                    .unwrap_or(rest.trim())
                    .to_string(),
            );
        }
    }
    flush(&mut cur_path, &mut cur_head, &mut cur_branch);
    Ok(out)
}

/// The git dir that distinguishes one worktree from its siblings.
///
/// A linked worktree has a `.git` FILE pointing at the real dir; the primary
/// checkout has a `.git` directory. Falling back to the repo's common dir keeps
/// the primary tree stable even when it cannot be probed.
fn worktree_git_dir(wt: &Path, common: Option<&str>) -> String {
    let dot_git = wt.join(".git");
    if dot_git.is_file() {
        if let Ok(text) = std::fs::read_to_string(&dot_git) {
            if let Some(rest) = text.trim().strip_prefix("gitdir:") {
                return rest.trim().to_string();
            }
        }
    }
    if dot_git.is_dir() {
        return dot_git.to_string_lossy().into_owned();
    }
    common.unwrap_or("").to_string()
}

// ---------------------------------------------------------------------------
// Root resolution — the half the A88 census showed matters most.
// ---------------------------------------------------------------------------

/// How a project's root path was established. Ordered by trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootVia {
    /// The vendor recorded this `cwd`. Authoritative — R04's rule, and the
    /// census measured the alternative wrong for 75% of the corpus.
    RecordedCwd,
    /// Rebuilt by walking the filesystem against the munged name, and exactly
    /// one candidate directory existed.
    ///
    /// There is deliberately no `Demangled` variant. An earlier draft accepted
    /// the lossy demangle whenever it happened to name a real directory, which
    /// is the very error the A88 census measured at 75% of the corpus: the
    /// demangle is only ever *a* candidate, and when a second candidate also
    /// exists, taking it is a guess wearing the costume of a resolution. The
    /// all-`/` reading is included in the walk below, so nothing is lost.
    Reconstructed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootResolution {
    Resolved {
        path: String,
        via: RootVia,
    },
    /// No path could be established, or reconstruction was ambiguous. The
    /// ambiguity is reported rather than resolved by picking one.
    Unresolved {
        reason: String,
    },
}

/// Mirror of `ingest::demangle`: `-Users-dev-x` → `/Users/dev/x`. Lossy exactly
/// where a real path component contained a non-alphanumeric character, which
/// the vendor also munged to `-`.
pub fn demangle_lossy(dir_name: &str) -> String {
    if dir_name.starts_with('-') {
        dir_name.replacen('-', "/", 1).replace('-', "/")
    } else {
        dir_name.to_string()
    }
}

/// Separators a `-` in a munged name might really have been. `/` is handled by
/// the walk itself.
const SEPARATOR_CANDIDATES: [&str; 4] = ["-", "_", ".", " "];

/// Rebuild a real path from a munged directory name by walking the filesystem.
///
/// The vendor maps every non-alphanumeric to `-`, so `Telescope_project` and
/// `Telescope/project` munge identically. Reconstruction therefore tries each
/// separator at each boundary and keeps only what actually exists on disk. When
/// more than one distinct path survives, it **abstains**: an ambiguous
/// reconstruction is exactly the guess the census caught being wrong.
pub fn reconstruct_root(dir_name: &str) -> RootResolution {
    let trimmed = dir_name.strip_prefix('-').unwrap_or(dir_name);
    let segments: Vec<&str> = trimmed.split('-').collect();
    if segments.is_empty() {
        return RootResolution::Unresolved {
            reason: "empty project directory name".to_string(),
        };
    }
    let mut frontier: Vec<(PathBuf, usize)> = vec![(PathBuf::from("/"), 0)];
    let mut complete: BTreeSet<String> = BTreeSet::new();
    // Bounded so a pathological name cannot walk the machine.
    let mut steps = 0usize;
    const MAX_STEPS: usize = 20_000;

    while let Some((base, idx)) = frontier.pop() {
        steps += 1;
        if steps > MAX_STEPS {
            return RootResolution::Unresolved {
                reason: "reconstruction exceeded its step budget".to_string(),
            };
        }
        if idx == segments.len() {
            complete.insert(base.to_string_lossy().into_owned());
            continue;
        }
        // Try consuming 1..n segments as ONE path component, joined by each
        // candidate separator, plus the plain `/` step of a single segment.
        for take in 1..=(segments.len() - idx) {
            let parts = &segments[idx..idx + take];
            let mut joined: BTreeSet<String> = BTreeSet::new();
            if take == 1 {
                joined.insert(parts[0].to_string());
            } else {
                for sep in SEPARATOR_CANDIDATES {
                    joined.insert(parts.join(sep));
                }
            }
            for name in joined {
                if name.is_empty() {
                    continue;
                }
                let candidate = base.join(&name);
                if candidate.is_dir() {
                    frontier.push((candidate, idx + take));
                }
            }
        }
    }

    match complete.len() {
        0 => RootResolution::Unresolved {
            reason: "no existing directory matches this project name".to_string(),
        },
        1 => {
            // `complete` is non-empty here, so the iterator yields.
            let only = complete.into_iter().next().unwrap_or_default();
            RootResolution::Resolved {
                path: only,
                via: RootVia::Reconstructed,
            }
        }
        n => RootResolution::Unresolved {
            reason: format!("{n} existing directories match this project name; abstaining"),
        },
    }
}

/// Resolve a project's real root. **`cwd` wins** — R04's rule, and the one the
/// census vindicated with a number.
pub fn resolve_project_root(dir_name: &str, recorded_cwd: Option<&str>) -> RootResolution {
    if let Some(cwd) = recorded_cwd {
        let cwd = cwd.trim();
        if !cwd.is_empty() && Path::new(cwd).is_dir() {
            return RootResolution::Resolved {
                path: cwd.to_string(),
                via: RootVia::RecordedCwd,
            };
        }
    }
    reconstruct_root(dir_name)
}

impl RootVia {
    /// The stored form. Exists so the `resolved_via` CHECK domain in the schema
    /// and this enum cannot drift: without it the schema pins string literals
    /// nothing can keep in sync, and a typo becomes a runtime INSERT failure
    /// mid-backfill instead of a compile error.
    pub fn as_str(self) -> &'static str {
        match self {
            RootVia::RecordedCwd => "recorded_cwd",
            RootVia::Reconstructed => "reconstructed",
        }
    }
}

/// One derived identity row, exactly as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIdentityRow {
    pub project_id: String,
    /// `None` is abstention, never a placeholder.
    pub repo_id: Option<String>,
    pub basis: String,
    pub root_commits_json: String,
    pub remote_fingerprints_json: String,
    pub resolved_root: Option<String>,
    pub resolved_via: Option<String>,
    /// How much evidence the resolution actually read. A resolution that cannot
    /// say how it was reached is the kind of unreceipted fact this project
    /// refuses.
    pub sessions_sampled: usize,
    pub observed_at: String,
}

fn json_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

/// Read the vendor-recorded `cwd` from the head of a transcript.
///
/// `cwd` is not a column in the ledger — it lives only in the raw JSONL, which
/// is the receipts path. Malformed lines are skipped, never guessed at.
///
/// **Two vendor shapes, both read.** Claude Code records `cwd` at the top level
/// of each line; codex records it inside `payload` (on `session_meta` and on
/// every `turn_context`). Reading only the first is why every codex project
/// derived `basis = "unresolvable"` and carried no `repo_id` — which silently
/// excluded all of them from A92 repo grouping, so a repo entered by both
/// agents grouped as the Claude half alone. Found by the L8 walk (A116).
fn cwd_in_transcript(path: &Path, lines_to_read: usize) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines().take(lines_to_read) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let candidate = value
            .get("cwd")
            .or_else(|| value.get("payload").and_then(|p| p.get("cwd")));
        if let Some(cwd) = candidate.and_then(|c| c.as_str()) {
            if !cwd.trim().is_empty() {
                return Some(cwd.to_string());
            }
        }
    }
    None
}

/// The newest still-present working directory the captured transcripts name.
///
/// This is deliberately narrower than [`resolve_project_root`]. A caller that
/// is about to observe a Change needs a root the capture itself named, not a
/// reconstruction from Claude's lossy directory key. A missing newest cwd does
/// not make an older, still-present captured cwd untrue, so keep looking.
pub fn existing_recorded_cwd(session_files: &[String], lines_to_read: usize) -> Option<PathBuf> {
    for file in session_files {
        if let Some(cwd) = cwd_in_transcript(Path::new(file), lines_to_read) {
            let path = PathBuf::from(cwd.trim());
            if path.is_dir() {
                return Some(path);
            }
        }
    }
    None
}

/// The one root this project has actually established on this machine.
///
/// Claude's directory key is lossy, so `projects.root_path` is not evidence of
/// a Claude root even when that path exists. Captured cwds and identity rows
/// marked `recorded_cwd` are evidence; disagreeing evidence abstains rather
/// than selecting whichever transcript happened to be newest. Codex records an
/// exact normalized route, so its existing root remains a compatibility
/// fallback. `explicit` is a person's `change new --root` selection and never
/// falls through when it is absent or invalid.
pub fn established_project_root(
    ledger: &Ledger,
    project_id: &str,
    explicit: Option<&Path>,
) -> Result<Option<PathBuf>> {
    if let Some(root) = explicit {
        return Ok(root.is_dir().then(|| canonical_root(root)));
    }
    let project: Option<(String, String)> = ledger
        .conn()
        .query_row(
            "SELECT source, root_path FROM projects WHERE id=?1",
            params![project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((source, stored_root)) = project else {
        return Ok(None);
    };

    let cached: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT resolved_root FROM workspace_identities
          WHERE project_id=?1 AND resolved_via='recorded_cwd'",
            params![project_id],
            |row| row.get(0),
        )
        .optional()?;
    let files = ledger.recent_session_files(project_id, 16)?;
    let mut roots = BTreeSet::new();
    if let Some(root) = cached.filter(|r| Path::new(r).is_dir()) {
        roots.insert(canonical_root(Path::new(&root)));
    }
    for file in files {
        if let Some(cwd) = cwd_in_transcript(Path::new(&file), 3) {
            let root = Path::new(cwd.trim());
            if root.is_dir() {
                roots.insert(canonical_root(root));
            }
        }
    }
    if roots.len() == 1 {
        return Ok(roots.into_iter().next());
    }
    if roots.len() > 1 {
        return Ok(None);
    }
    if source == "codex" && Path::new(&stored_root).is_dir() {
        return Ok(Some(canonical_root(Path::new(&stored_root))));
    }
    Ok(None)
}

fn canonical_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// Derive one project's identity row from its transcripts and the filesystem.
///
/// Additive and abstaining: a project whose directory is gone yields
/// `basis = "unresolvable"` with no `repo_id`. `project_id` is never touched —
/// it is the chain genesis.
pub fn identity_for_project(
    project_id: &str,
    session_files: &[String],
    lines_per_file: usize,
    observed_at: &str,
) -> Result<WorkspaceIdentityRow> {
    let mut sampled = 0usize;
    let mut recorded_cwd = None;
    for file in session_files {
        sampled += 1;
        if let Some(cwd) = cwd_in_transcript(Path::new(file), lines_per_file) {
            recorded_cwd = Some(cwd);
            break;
        }
    }

    let resolution = resolve_project_root(project_id, recorded_cwd.as_deref());
    let (resolved_root, resolved_via) = match &resolution {
        RootResolution::Resolved { path, via } => (Some(path.clone()), Some(via.as_str())),
        RootResolution::Unresolved { .. } => (None, None),
    };

    let identity = match &resolved_root {
        Some(path) => repo_identity_at(Path::new(path))?,
        None => RepoIdentity::unresolvable(),
    };

    Ok(WorkspaceIdentityRow {
        project_id: project_id.to_string(),
        repo_id: identity.repo_id,
        basis: identity.basis.as_str().to_string(),
        root_commits_json: json_array(&identity.root_commits),
        remote_fingerprints_json: json_array(&identity.remote_fingerprints),
        resolved_root,
        resolved_via: resolved_via.map(str::to_string),
        sessions_sampled: sampled,
        observed_at: observed_at.to_string(),
    })
}

/// One repo, and every project that turned out to be a working root inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoGroup {
    pub repo_id: String,
    pub roots: Vec<RepoGroupRoot>,
}

/// A project bound to a repo. `working_root` is the directory the agent was
/// actually launched from, which for a subdirectory launch is NOT the repo
/// root — the A88 census found eleven project ids that were one repo entered at
/// eleven different working roots, and flattening them to the repo root would
/// discard the only thing distinguishing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoGroupRoot {
    pub project_id: String,
    pub working_root: Option<String>,
}

/// Group identity rows by repo. **Projections only — nothing merges.**
///
/// Each `project_id` keeps its own row, its own chain, and its own history.
/// Grouping is how one repo entered at many working roots is *displayed*; it is
/// never a mutation, because merging projects would fuse chains, and the chain
/// is the one structure the ledger exists to protect.
///
/// Rows that abstain (`repo_id: None`) are deliberately absent from every
/// group: an unresolvable project belongs to no repo we can prove, and putting
/// it anywhere would be a guess.
pub fn group_by_repo(rows: &[WorkspaceIdentityRow]) -> Vec<RepoGroup> {
    let mut by_repo: std::collections::BTreeMap<String, Vec<RepoGroupRoot>> =
        std::collections::BTreeMap::new();
    for row in rows {
        let Some(repo_id) = row.repo_id.clone() else {
            continue;
        };
        by_repo.entry(repo_id).or_default().push(RepoGroupRoot {
            project_id: row.project_id.clone(),
            working_root: row.resolved_root.clone(),
        });
    }
    by_repo
        .into_iter()
        .map(|(repo_id, mut roots)| {
            roots.sort_by(|a, b| a.project_id.cmp(&b.project_id));
            RepoGroup { repo_id, roots }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_normalisation_unifies_ssh_and_https() {
        let https = normalise_remote("https://github.com/Org/repo.git");
        let ssh = normalise_remote("git@github.com:Org/repo.git");
        let bare = normalise_remote("https://GitHub.com/Org/repo/");
        assert_eq!(https, "github.com/Org/repo");
        assert_eq!(ssh, https, "same repo fetched two ways must agree");
        assert_eq!(bare, https, "host case and trailing slash are not identity");
        assert_eq!(remote_fingerprint(&https), remote_fingerprint(&ssh));
    }

    #[test]
    fn different_repos_do_not_share_a_fingerprint() {
        assert_ne!(
            remote_fingerprint("https://github.com/Org/a"),
            remote_fingerprint("https://github.com/Org/b")
        );
    }

    #[test]
    fn a_port_is_dropped_but_an_scp_path_is_kept() {
        // A port is transport, not identity.
        assert_eq!(
            normalise_remote("ssh://git@host:22/org/repo.git"),
            "host/org/repo"
        );
        assert_eq!(
            normalise_remote("ssh://git@host:22/org/repo.git"),
            normalise_remote("git@host:org/repo.git"),
            "the same repo over ssh with and without an explicit port"
        );
        // Not a port: the colon introduces a path.
        assert_eq!(normalise_remote("git@host:22org/repo"), "host/22org/repo");
    }

    #[test]
    fn repo_id_is_order_independent_and_collision_resistant() {
        let a = repo_id_from(&["r1".into(), "r2".into()], &["f1".into()]);
        let b = repo_id_from(&["r2".into(), "r1".into()], &["f1".into()]);
        assert_eq!(a, b, "input order must not change identity");

        // Length prefixing: concatenation ambiguity must not collide.
        let x = repo_id_from(&["ab".into(), "c".into()], &[]);
        let y = repo_id_from(&["a".into(), "bc".into()], &[]);
        assert_ne!(x, y);

        // The two inputs are in distinct positions.
        assert_ne!(
            repo_id_from(&["a".into()], &[]),
            repo_id_from(&[], &["a".into()])
        );
    }

    #[test]
    fn worktree_ids_differ_within_one_repo() {
        let repo = repo_id_from(&["root".into()], &[]);
        let a = worktree_id_from(&repo, "/r/.git");
        let b = worktree_id_from(&repo, "/r/.git/worktrees/feature");
        assert_ne!(a, b, "two worktrees of one repo must be distinguishable");
        assert_eq!(
            a,
            worktree_id_from(&repo, "/r/.git"),
            "derivation is stable"
        );
    }

    #[test]
    fn hashes_are_lowercase_hex_of_the_expected_width() {
        let id = repo_id_from(&["root".into()], &[]);
        assert_eq!(id.len(), 64);
        assert!(id
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    }

    #[test]
    fn demangle_matches_the_ingest_form() {
        assert_eq!(demangle_lossy("-Users-dev-x"), "/Users/dev/x");
        assert_eq!(demangle_lossy("plain"), "plain");
    }

    #[test]
    fn unresolvable_identity_carries_no_id() {
        let missing = repo_identity_at(Path::new("/nonexistent/thruline/probe"));
        let id = missing.unwrap_or_else(|_| RepoIdentity::unresolvable());
        assert_eq!(id.basis, Basis::Unresolvable);
        assert!(id.repo_id.is_none(), "abstention must not mint an id");
    }

    #[test]
    fn recorded_cwd_beats_the_demangled_name() {
        // `/tmp` exists; the demangle of this name does not.
        let r = resolve_project_root("-no-such-path-anywhere-xyz", Some("/tmp"));
        assert_eq!(
            r,
            RootResolution::Resolved {
                path: "/tmp".to_string(),
                via: RootVia::RecordedCwd
            }
        );
    }

    #[test]
    fn a_dead_cwd_does_not_win() {
        let r = resolve_project_root("-tmp", Some("/nonexistent/dead/cwd"));
        match r {
            RootResolution::Resolved { path, via } => {
                assert_eq!(path, "/tmp");
                assert_eq!(via, RootVia::Reconstructed);
            }
            other => panic!("expected reconstruction to take over, got {other:?}"),
        }
    }

    #[test]
    fn an_unresolvable_project_says_so() {
        let r = resolve_project_root("-nonexistent-thruline-probe-zzz", None);
        assert!(matches!(r, RootResolution::Unresolved { .. }));
    }
}
