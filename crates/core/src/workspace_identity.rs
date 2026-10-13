//! MANDATE-v4 §12 (A206) — the canonical workspace identity, `WSID_V1`.
//!
//! One versioned, deterministic fingerprint over everything §12 says a
//! workspace's identity must account for: stable repository identity (reused
//! from [`crate::workspace`], never re-derived here), canonical root, git
//! common directory and worktree identity, branch or detached-HEAD state,
//! HEAD, the index as its own axis, staged / modified / deleted / renamed
//! tracked state, merge conflicts, file modes and executable bits, symlink
//! targets, relevant untracked files under a declared scope, submodule state,
//! sparse-checkout and LFS attribution where present, declared exclusions,
//! and completeness of observation.
//!
//! Four laws:
//!
//! 1. **Read-only, structurally.** Every subprocess goes through the guarded
//!    plumbing runner in [`crate::capture`] (`GIT_OPTIONAL_LOCKS=0`, hooks
//!    and user config neutralized), and the only verbs this module speaks
//!    are `status --porcelain=v2 -z`, `ls-files -s`, `rev-parse`,
//!    `symbolic-ref`, and `submodule status`. A test walks this file for
//!    mutating verbs and for any direct process spawn, and fails on either.
//!    Textual `git status` output is never the source of truth — plumbing
//!    formats only (§12).
//! 2. **Deterministic, or refused.** The same tree state canonicalizes to the
//!    same bytes and the same digest, on any machine. Nothing time-, locale-,
//!    or environment-dependent enters the canonical form.
//! 3. **The freeze rule.** Scope and exclusions are declared before
//!    observation, normalized, and hashed into the document itself. The
//!    comparison API takes no scope parameter — the observed side is captured
//!    under the *expected* side's frozen scope — so changing scope after a
//!    mismatch is not a misuse to detect but a call that cannot be written.
//! 4. **Incomplete never matches.** The porcelain state and HEAD are read
//!    before and after everything else; if they moved, the observation is
//!    [`Observation::Incomplete`], which carries no digest at all — the type
//!    system, not a runtime check, is what makes a match unreachable (§12:
//!    never claim a match from an incomplete observation).
//!
//!    The sentinel's declared boundary, stated rather than hidden: it sees
//!    every mutation that changes the porcelain byte stream or HEAD between
//!    the two reads. A write that leaves both byte-identical — new content
//!    inside a file that was already dirty at both edges — is invisible to
//!    it; the content digests then read the file's final bytes, so the
//!    document still describes the tree as it stood at the second read for
//!    that file. Closing even that window (a second content pass over the
//!    dirty set) is recorded as future hardening in the A206 register
//!    entry, not silently assumed away.
//!
//! ## The WSID_V1 canonical layout
//!
//! The encoding follows [`crate::chain`]'s injective conventions: every
//! string is length-prefixed (`u64` little-endian byte length, then UTF-8
//! bytes, written `lp(s)`), every optional field is one tag byte (`0x00`
//! absent, `0x01` present followed by `lp`), every list is a `u64`
//! little-endian count followed by its items in the documented sort order.
//! The document, in order:
//!
//! ```text
//! lp("WSID_V1")
//! lp("scope"); include_untracked byte; u64 count; lp(exclusion)…   (byte-sorted, deduped)
//! lp("repo"); opt(repo_id)
//! lp("root"); lp(canonical_root)
//! lp("worktree"); lp(git_common_dir); opt(worktree_id)
//! lp("branch-state"); opt(branch)
//! lp("head"); opt(head_commit)
//! lp("index"); lp(index_digest)
//! lp("staged");     list: lp(path) lp(head_mode) lp(index_mode) lp(index_object)
//! lp("dirty");      list: lp(path) lp(worktree_mode) lp(worktree_sha256)
//! lp("deleted");    list: lp(path)
//! lp("renamed");    list: lp(new_path) lp(old_path) lp(head_mode) lp(index_mode) lp(index_object)
//! lp("conflicts");  list: lp(path) lp(m1) lp(m2) lp(m3) lp(h1) lp(h2) lp(h3)
//! lp("untracked");  list: lp(path) lp(sha256)
//! lp("symlinks");   list: lp(path) lp(target)
//! lp("submodules"); list: lp(path) lp(state) lp(head)
//! lp("sparse"); opt(sparse_digest)
//! lp("lfs"); opt(lfs_digest)
//! lp("complete")
//! ```
//!
//! The digest is the SHA-256 of those bytes, lowercase hex; the identity
//! string stored in observation envelopes is `wsid1:<digest>` — the string
//! carries its own version tag because the envelope's `workspace_identity`
//! column is free-form (A205 handoff). `index_digest` is itself the SHA-256
//! over, for each `git ls-files -s` entry in git's own output order:
//! `lp(mode) lp(object) u64(stage) lp(path)`. The trailing `lp("complete")`
//! binds the completeness assertion into the preimage: only a complete
//! observation is ever canonicalized, so every WSID_V1 digest *is* a claim
//! of complete observation. Changing any of this is a versioned migration to
//! `WSID_V2`, never a refactor — golden vectors computed outside this code
//! pin the layout.
//!
//! ## What each section speaks for
//!
//! * **dirty** — tracked paths whose worktree differs from the index
//!   (content, type, or mode: a bare `chmod +x` lands here with its new
//!   `worktree_mode`). `worktree_sha256` digests the worktree bytes; for a
//!   symlink it digests the target path's UTF-8 bytes, and the target itself
//!   is also recorded in **symlinks**.
//! * **untracked** — files `status --porcelain=v2 -uall` reports, which is
//!   the same boundary the snapshotter froze: `.gitignore`d files are outside
//!   the fingerprint by construction, and the declared exclusions narrow it
//!   further (byte-prefix match against repo-relative paths). Submodule
//!   worktrees are spoken for by **submodules** (state character and
//!   checked-out HEAD), never content-walked.
//! * **sparse / lfs** — present-where-applicable digests: the
//!   `info/sparse-checkout` file bytes, and the root `.gitattributes` bytes
//!   when they declare an `lfs` filter. LFS pointer *content* is already
//!   covered by the index and dirty axes — pointers are the tracked bytes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::capture::{git_optional, git_output, trusted_git};
use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::observation::{
    append, AppendOutcome, Completeness, DerivedRef, NewObservation, ObservationType,
    ReceiptTerminal, SourceReceipt,
};

/// The version literal inside the canonical document.
pub const WSID_VERSION: &str = "WSID_V1";

/// The tag on the identity string an observation envelope stores.
pub const IDENTITY_PREFIX: &str = "wsid1:";

/// The declared observation scope — frozen into the document before any
/// comparison can happen (§12: scope and exclusions are established before
/// comparison and cannot change after a mismatch is observed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// Whether relevant untracked files are part of this identity at all.
    pub include_untracked: bool,
    /// Repo-relative byte-prefix exclusions applied to untracked paths
    /// (e.g. `"target/"`). Normalized — sorted and deduped — at observation.
    pub exclusions: Vec<String>,
}

impl Scope {
    /// The default scope, matching the snapshotter's frozen convention:
    /// untracked files are in, `.gitignore` is the only implicit exclusion.
    pub fn everything() -> Self {
        Scope {
            include_untracked: true,
            exclusions: Vec::new(),
        }
    }
}

/// One tracked path whose index entry differs from HEAD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedEntry {
    pub path: String,
    pub head_mode: String,
    pub index_mode: String,
    pub index_object: String,
}

/// One tracked path whose worktree differs from the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyEntry {
    pub path: String,
    pub worktree_mode: String,
    pub worktree_sha256: String,
}

/// One staged rename (or copy), old name preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamedEntry {
    pub new_path: String,
    pub old_path: String,
    pub head_mode: String,
    pub index_mode: String,
    pub index_object: String,
}

/// One unmerged path: the three stage modes and objects, verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictEntry {
    pub path: String,
    pub modes: [String; 3],
    pub objects: [String; 3],
}

/// One relevant untracked file inside the declared scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrackedEntry {
    pub path: String,
    pub sha256: String,
}

/// One worktree symlink (changed or untracked), with its target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymlinkEntry {
    pub path: String,
    pub target: String,
}

/// One submodule: porcelain state character and checked-out HEAD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmoduleEntry {
    pub path: String,
    pub state: String,
    pub head: String,
}

/// The full canonical document. Fields are public so the workspace-oracle
/// harness can build documents against the pinned external vectors; a
/// [`WorkspaceFingerprint`] — the only thing [`compare`] accepts as an
/// expectation — can still only be minted by [`observe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDoc {
    pub scope: Scope,
    /// The stable repository identity from [`crate::workspace`]; `None` is
    /// that layer's abstention, carried through — never a placeholder.
    pub repo_id: Option<String>,
    pub root: String,
    pub git_common_dir: String,
    pub worktree_id: Option<String>,
    /// `None` is detached or unborn; `head` disambiguates.
    pub branch: Option<String>,
    pub head: Option<String>,
    pub index_digest: String,
    pub staged: Vec<StagedEntry>,
    pub dirty: Vec<DirtyEntry>,
    pub deleted: Vec<String>,
    pub renamed: Vec<RenamedEntry>,
    pub conflicts: Vec<ConflictEntry>,
    pub untracked: Vec<UntrackedEntry>,
    pub symlinks: Vec<SymlinkEntry>,
    pub submodules: Vec<SubmoduleEntry>,
    pub sparse_digest: Option<String>,
    pub lfs_digest: Option<String>,
}

/// A complete observation's identity: the document plus its tagged digest.
/// Fields are private on purpose — the only constructors are [`observe`] /
/// [`observe_with`], so an expectation handed to [`compare`] is always a
/// really-observed, complete fingerprint with its scope frozen inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFingerprint {
    doc: WorkspaceDoc,
    identity: String,
}

impl WorkspaceFingerprint {
    pub fn doc(&self) -> &WorkspaceDoc {
        &self.doc
    }

    /// The tagged identity string, `wsid1:<64 hex>`.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The raw digest, without the version tag.
    pub fn digest_hex(&self) -> &str {
        self.identity
            .strip_prefix(IDENTITY_PREFIX)
            .unwrap_or(&self.identity)
    }
}

/// What an observation attempt produced. `Incomplete` carries **no digest
/// and no document** — an interrupted observation has nothing a comparison
/// could accept, so "incomplete never matches" is a property of the type,
/// not of caller discipline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    Complete(Box<WorkspaceFingerprint>),
    Incomplete {
        reason: String,
        /// The repository identity, when it was resolved before the
        /// interruption — so the incomplete observation can still be
        /// recorded against the right repository.
        repo_id: Option<String>,
        root: String,
    },
}

/* ── canonical encoding ─────────────────────────────────────────────── */

fn push_lp(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn push_opt(out: &mut Vec<u8>, v: Option<&str>) {
    match v {
        None => out.push(0x00),
        Some(s) => {
            out.push(0x01);
            push_lp(out, s);
        }
    }
}

fn push_count(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u64).to_le_bytes());
}

/// The documented WSID_V1 canonical bytes of a document, exactly as given —
/// [`observe`] is responsible for sorting lists and normalizing scope before
/// this is called. Golden vectors computed outside Rust pin this layout.
pub fn canonical_bytes(doc: &WorkspaceDoc) -> Vec<u8> {
    let mut out = Vec::with_capacity(1024);
    push_lp(&mut out, WSID_VERSION);
    push_lp(&mut out, "scope");
    out.push(u8::from(doc.scope.include_untracked));
    push_count(&mut out, doc.scope.exclusions.len());
    for e in &doc.scope.exclusions {
        push_lp(&mut out, e);
    }
    push_lp(&mut out, "repo");
    push_opt(&mut out, doc.repo_id.as_deref());
    push_lp(&mut out, "root");
    push_lp(&mut out, &doc.root);
    push_lp(&mut out, "worktree");
    push_lp(&mut out, &doc.git_common_dir);
    push_opt(&mut out, doc.worktree_id.as_deref());
    push_lp(&mut out, "branch-state");
    push_opt(&mut out, doc.branch.as_deref());
    push_lp(&mut out, "head");
    push_opt(&mut out, doc.head.as_deref());
    push_lp(&mut out, "index");
    push_lp(&mut out, &doc.index_digest);
    push_lp(&mut out, "staged");
    push_count(&mut out, doc.staged.len());
    for s in &doc.staged {
        push_lp(&mut out, &s.path);
        push_lp(&mut out, &s.head_mode);
        push_lp(&mut out, &s.index_mode);
        push_lp(&mut out, &s.index_object);
    }
    push_lp(&mut out, "dirty");
    push_count(&mut out, doc.dirty.len());
    for d in &doc.dirty {
        push_lp(&mut out, &d.path);
        push_lp(&mut out, &d.worktree_mode);
        push_lp(&mut out, &d.worktree_sha256);
    }
    push_lp(&mut out, "deleted");
    push_count(&mut out, doc.deleted.len());
    for p in &doc.deleted {
        push_lp(&mut out, p);
    }
    push_lp(&mut out, "renamed");
    push_count(&mut out, doc.renamed.len());
    for r in &doc.renamed {
        push_lp(&mut out, &r.new_path);
        push_lp(&mut out, &r.old_path);
        push_lp(&mut out, &r.head_mode);
        push_lp(&mut out, &r.index_mode);
        push_lp(&mut out, &r.index_object);
    }
    push_lp(&mut out, "conflicts");
    push_count(&mut out, doc.conflicts.len());
    for c in &doc.conflicts {
        push_lp(&mut out, &c.path);
        for m in &c.modes {
            push_lp(&mut out, m);
        }
        for o in &c.objects {
            push_lp(&mut out, o);
        }
    }
    push_lp(&mut out, "untracked");
    push_count(&mut out, doc.untracked.len());
    for u in &doc.untracked {
        push_lp(&mut out, &u.path);
        push_lp(&mut out, &u.sha256);
    }
    push_lp(&mut out, "symlinks");
    push_count(&mut out, doc.symlinks.len());
    for s in &doc.symlinks {
        push_lp(&mut out, &s.path);
        push_lp(&mut out, &s.target);
    }
    push_lp(&mut out, "submodules");
    push_count(&mut out, doc.submodules.len());
    for s in &doc.submodules {
        push_lp(&mut out, &s.path);
        push_lp(&mut out, &s.state);
        push_lp(&mut out, &s.head);
    }
    push_lp(&mut out, "sparse");
    push_opt(&mut out, doc.sparse_digest.as_deref());
    push_lp(&mut out, "lfs");
    push_opt(&mut out, doc.lfs_digest.as_deref());
    push_lp(&mut out, "complete");
    out
}

/// The raw WSID_V1 digest of a document — SHA-256 of [`canonical_bytes`].
pub fn digest_of(doc: &WorkspaceDoc) -> String {
    sha256_hex(&canonical_bytes(doc))
}

/// The tagged identity string, `wsid1:<digest>`.
pub fn identity_of(doc: &WorkspaceDoc) -> String {
    format!("{IDENTITY_PREFIX}{}", digest_of(doc))
}

/* ── observation ────────────────────────────────────────────────────── */

const STATUS_ARGS: &[&str] = &["status", "--porcelain=v2", "-z", "--untracked-files=all"];

/// What the porcelain parse yields before any file content is read.
#[derive(Default)]
struct Skeleton {
    staged: Vec<StagedEntry>,
    /// (path, worktree_mode) pairs still needing a content digest.
    dirty_pending: Vec<(String, String)>,
    deleted: Vec<String>,
    renamed: Vec<RenamedEntry>,
    conflicts: Vec<ConflictEntry>,
    untracked_pending: Vec<String>,
}

fn parse_error(record: &str) -> CoreError {
    CoreError::Integrity(format!(
        "unrecognized porcelain v2 record {record:?} — refusing to fingerprint around it"
    ))
}

/// Parse `status --porcelain=v2 -z` bytes. Every record is either understood
/// or a loud error: a fingerprint that skipped a record it could not read
/// would claim completeness it does not have.
fn parse_porcelain_v2(bytes: &[u8]) -> Result<Skeleton> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Skeleton::default();
    let mut tokens = text.split('\0').filter(|t| !t.is_empty());
    while let Some(tok) = tokens.next() {
        if let Some(rest) = tok.strip_prefix("? ") {
            out.untracked_pending.push(rest.to_string());
            continue;
        }
        if tok.starts_with("# ") || tok.starts_with("! ") {
            continue;
        }
        if tok.starts_with("1 ") {
            let f: Vec<&str> = tok.splitn(9, ' ').collect();
            let [_, xy, sub, m_h, m_i, m_w, _h_h, h_i, path] = f.as_slice() else {
                return Err(parse_error(tok));
            };
            let (x, y) = xy_of(xy).ok_or_else(|| parse_error(tok))?;
            let is_submodule = sub.starts_with('S');
            if x == 'D' || y == 'D' {
                out.deleted.push((*path).to_string());
            }
            if x != '.' && x != 'D' {
                out.staged.push(StagedEntry {
                    path: (*path).to_string(),
                    head_mode: (*m_h).to_string(),
                    index_mode: (*m_i).to_string(),
                    index_object: (*h_i).to_string(),
                });
            }
            if y != '.' && y != 'D' && !is_submodule {
                out.dirty_pending
                    .push(((*path).to_string(), (*m_w).to_string()));
            }
            continue;
        }
        if tok.starts_with("2 ") {
            let f: Vec<&str> = tok.splitn(10, ' ').collect();
            let [_, xy, sub, m_h, m_i, m_w, _h_h, h_i, _score, new_path] = f.as_slice() else {
                return Err(parse_error(tok));
            };
            let old_path = tokens.next().ok_or_else(|| parse_error(tok))?;
            let (_, y) = xy_of(xy).ok_or_else(|| parse_error(tok))?;
            out.renamed.push(RenamedEntry {
                new_path: (*new_path).to_string(),
                old_path: old_path.to_string(),
                head_mode: (*m_h).to_string(),
                index_mode: (*m_i).to_string(),
                index_object: (*h_i).to_string(),
            });
            if y == 'D' {
                out.deleted.push((*new_path).to_string());
            } else if y != '.' && !sub.starts_with('S') {
                out.dirty_pending
                    .push(((*new_path).to_string(), (*m_w).to_string()));
            }
            continue;
        }
        if tok.starts_with("u ") {
            let f: Vec<&str> = tok.splitn(11, ' ').collect();
            let [_, _xy, _sub, m1, m2, m3, _mw, h1, h2, h3, path] = f.as_slice() else {
                return Err(parse_error(tok));
            };
            out.conflicts.push(ConflictEntry {
                path: (*path).to_string(),
                modes: [(*m1).to_string(), (*m2).to_string(), (*m3).to_string()],
                objects: [(*h1).to_string(), (*h2).to_string(), (*h3).to_string()],
            });
            continue;
        }
        return Err(parse_error(tok));
    }
    Ok(out)
}

fn xy_of(xy: &str) -> Option<(char, char)> {
    let mut it = xy.chars();
    match (it.next(), it.next(), it.next()) {
        (Some(x), Some(y), None) => Some((x, y)),
        _ => None,
    }
}

/// Digest one worktree path. A symlink digests its target string (reading
/// through it would hash the wrong object's bytes) and reports the target.
fn digest_worktree(base: &Path, rel: &str) -> std::io::Result<(String, Option<String>)> {
    let full = base.join(rel);
    let meta = std::fs::symlink_metadata(&full)?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(&full)?.to_string_lossy().into_owned();
        Ok((sha256_hex(target.as_bytes()), Some(target)))
    } else {
        let bytes = std::fs::read(&full)?;
        Ok((sha256_hex(&bytes), None))
    }
}

fn canonical_path_text(p: &Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .display()
        .to_string()
}

fn absolutize(base: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        base.join(p)
    }
}

/// Observe the workspace containing `root` under a declared scope.
pub fn observe(root: &Path, scope: Scope) -> Result<Observation> {
    observe_with(root, scope, || {})
}

/// The observation with a test seam: `between` runs after the first
/// porcelain/HEAD read and before everything else — the same seam the
/// snapshotter carries, so the concurrent-mutation law is testable with a
/// real interleaved write rather than a timing simulation. The public
/// [`observe`] passes a no-op.
pub fn observe_with(root: &Path, scope: Scope, between: impl FnOnce()) -> Result<Observation> {
    let git = trusted_git()?;
    let Some(toplevel) = git_optional(&git, root, &["rev-parse", "--show-toplevel"])? else {
        return Err(CoreError::Config(format!(
            "{} is not inside a git worktree — {WSID_VERSION} speaks only for git workspaces",
            root.display()
        )));
    };
    let base = PathBuf::from(&toplevel);
    let canonical_root = canonical_path_text(&base);

    // The concurrent-mutation sentinel's left edge: porcelain and HEAD,
    // before anything else is read.
    let pre_head = git_optional(&git, &base, &["rev-parse", "HEAD"])?;
    let pre_porcelain = git_output(&git, &base, STATUS_ARGS)?;

    between();

    // Repository and worktree identity — reused, not re-derived.
    let repo = crate::workspace::repo_identity_at(&base)?;
    let git_common_dir = git_optional(&git, &base, &["rev-parse", "--git-common-dir"])?
        .map(|d| canonical_path_text(&absolutize(&base, &d)))
        .unwrap_or_default();
    let git_dir = git_optional(&git, &base, &["rev-parse", "--absolute-git-dir"])?
        .map(|d| canonical_path_text(&absolutize(&base, &d)))
        .unwrap_or_default();
    let worktree_id = repo
        .repo_id
        .as_ref()
        .map(|id| crate::workspace::worktree_id_from(id, &git_dir));
    let branch = git_optional(&git, &base, &["symbolic-ref", "--short", "-q", "HEAD"])?;

    // The index, as its own axis: every tracked entry's mode, object, stage.
    let ls = git_output(&git, &base, &["ls-files", "-s", "-z", "--full-name"])?;
    let ls_text = String::from_utf8_lossy(&ls);
    let mut index_bytes = Vec::new();
    let mut gitlinks: BTreeMap<String, String> = BTreeMap::new();
    for entry in ls_text.split('\0').filter(|t| !t.is_empty()) {
        let Some((meta, path)) = entry.split_once('\t') else {
            return Err(CoreError::Integrity(format!(
                "unrecognized ls-files entry {entry:?}"
            )));
        };
        let fields: Vec<&str> = meta.split(' ').collect();
        let [mode, object, stage] = fields.as_slice() else {
            return Err(CoreError::Integrity(format!(
                "unrecognized ls-files entry {entry:?}"
            )));
        };
        let stage_n: u64 = stage.parse().map_err(|_| {
            CoreError::Integrity(format!("unrecognized ls-files stage in {entry:?}"))
        })?;
        push_lp(&mut index_bytes, mode);
        push_lp(&mut index_bytes, object);
        index_bytes.extend_from_slice(&stage_n.to_le_bytes());
        push_lp(&mut index_bytes, path);
        if *mode == "160000" {
            gitlinks.insert(path.to_string(), (*object).to_string());
        }
    }
    let index_digest = sha256_hex(&index_bytes);

    // Tree state from the porcelain bytes already in hand.
    let skeleton = parse_porcelain_v2(&pre_porcelain)?;

    // Normalize the declared scope BEFORE it is frozen into the document, so
    // reordered exclusion lists are nuisance, not identity.
    let mut exclusions = scope.exclusions;
    exclusions.sort();
    exclusions.dedup();
    let scope = Scope {
        include_untracked: scope.include_untracked,
        exclusions,
    };

    let incomplete = |reason: String| Observation::Incomplete {
        reason,
        repo_id: repo.repo_id.clone(),
        root: canonical_root.clone(),
    };

    // Content digests — the only phase that reads worktree bytes.
    let mut dirty = Vec::new();
    let mut symlinks = Vec::new();
    for (path, mode) in &skeleton.dirty_pending {
        match digest_worktree(&base, path) {
            Ok((sha, target)) => {
                dirty.push(DirtyEntry {
                    path: path.clone(),
                    worktree_mode: mode.clone(),
                    worktree_sha256: sha,
                });
                if let Some(target) = target {
                    symlinks.push(SymlinkEntry {
                        path: path.clone(),
                        target,
                    });
                }
            }
            Err(e) => {
                return Ok(incomplete(format!(
                    "could not read {path} mid-observation ({e}) — the tree moved or refused"
                )))
            }
        }
    }
    let mut untracked = Vec::new();
    if scope.include_untracked {
        for path in &skeleton.untracked_pending {
            if scope.exclusions.iter().any(|ex| path.starts_with(ex)) {
                continue;
            }
            match digest_worktree(&base, path) {
                Ok((sha, target)) => {
                    untracked.push(UntrackedEntry {
                        path: path.clone(),
                        sha256: sha,
                    });
                    if let Some(target) = target {
                        symlinks.push(SymlinkEntry {
                            path: path.clone(),
                            target,
                        });
                    }
                }
                Err(e) => {
                    return Ok(incomplete(format!(
                        "could not read {path} mid-observation ({e}) — the tree moved or refused"
                    )))
                }
            }
        }
    }

    // Submodules: the index says where they are; `submodule status` says
    // what state each is in. A gitlink with no status line still appears,
    // with its index object and an unknown state — recorded, not dropped.
    let mut submodules = Vec::new();
    if !gitlinks.is_empty() {
        let status_text = git_optional(&git, &base, &["submodule", "status"])?.unwrap_or_default();
        let mut by_path: BTreeMap<String, (String, String)> = BTreeMap::new();
        for line in status_text.lines() {
            if line.len() < 42 {
                continue;
            }
            let state = &line[..1];
            let rest = &line[1..];
            if !rest.is_char_boundary(40) {
                continue;
            }
            let (sha, tail) = rest.split_at(40);
            let path_part = tail.trim_start();
            let path_clean = match path_part.rfind(" (") {
                Some(i) if path_part.ends_with(')') => &path_part[..i],
                _ => path_part,
            };
            by_path.insert(path_clean.to_string(), (state.to_string(), sha.to_string()));
        }
        for (path, index_object) in &gitlinks {
            let (state, head) = by_path
                .get(path)
                .cloned()
                .unwrap_or_else(|| ("?".to_string(), index_object.clone()));
            submodules.push(SubmoduleEntry {
                path: path.clone(),
                state,
                head,
            });
        }
    }

    // Sparse-checkout and LFS attribution, where present.
    let sparse_digest = git_optional(
        &git,
        &base,
        &["rev-parse", "--git-path", "info/sparse-checkout"],
    )?
    .and_then(|p| std::fs::read(absolutize(&base, &p)).ok())
    .map(|bytes| sha256_hex(&bytes));
    let lfs_digest = std::fs::read(base.join(".gitattributes"))
        .ok()
        .filter(|bytes| String::from_utf8_lossy(bytes).contains("filter=lfs"))
        .map(|bytes| sha256_hex(&bytes));

    // The sentinel's right edge: if porcelain or HEAD moved, everything read
    // in between describes no single moment, and says so.
    let post_head = git_optional(&git, &base, &["rev-parse", "HEAD"])?;
    let post_porcelain = git_output(&git, &base, STATUS_ARGS)?;
    if pre_head != post_head || pre_porcelain != post_porcelain {
        return Ok(incomplete(
            "the workspace changed during observation — capture again".to_string(),
        ));
    }

    let mut staged = skeleton.staged;
    staged.sort_by(|a, b| a.path.cmp(&b.path));
    dirty.sort_by(|a, b| a.path.cmp(&b.path));
    let mut deleted = skeleton.deleted;
    deleted.sort();
    deleted.dedup();
    let mut renamed = skeleton.renamed;
    renamed.sort_by(|a, b| a.new_path.cmp(&b.new_path));
    let mut conflicts = skeleton.conflicts;
    conflicts.sort_by(|a, b| a.path.cmp(&b.path));
    untracked.sort_by(|a, b| a.path.cmp(&b.path));
    symlinks.sort_by(|a, b| a.path.cmp(&b.path));

    let doc = WorkspaceDoc {
        scope,
        repo_id: repo.repo_id,
        root: canonical_root,
        git_common_dir,
        worktree_id,
        branch,
        head: pre_head,
        index_digest,
        staged,
        dirty,
        deleted,
        renamed,
        conflicts,
        untracked,
        symlinks,
        submodules,
        sparse_digest,
        lfs_digest,
    };
    let identity = identity_of(&doc);
    Ok(Observation::Complete(Box::new(WorkspaceFingerprint {
        doc,
        identity,
    })))
}

/* ── comparison ─────────────────────────────────────────────────────── */

/// One axis of workspace identity, for naming what mismatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    RepositoryIdentity,
    Root,
    WorktreeIdentity,
    BranchState,
    Head,
    Index,
    Staged,
    DirtyTracked,
    Deleted,
    Renamed,
    Conflicts,
    Modes,
    Symlinks,
    Untracked,
    Submodules,
    SparseCheckout,
    Lfs,
}

impl Axis {
    pub fn as_str(self) -> &'static str {
        match self {
            Axis::RepositoryIdentity => "repository-identity",
            Axis::Root => "root",
            Axis::WorktreeIdentity => "worktree-identity",
            Axis::BranchState => "branch-state",
            Axis::Head => "head",
            Axis::Index => "index",
            Axis::Staged => "staged",
            Axis::DirtyTracked => "dirty-tracked",
            Axis::Deleted => "deleted",
            Axis::Renamed => "renamed",
            Axis::Conflicts => "conflicts",
            Axis::Modes => "modes",
            Axis::Symlinks => "symlinks",
            Axis::Untracked => "untracked",
            Axis::Submodules => "submodules",
            Axis::SparseCheckout => "sparse",
            Axis::Lfs => "lfs",
        }
    }
}

/// The outcome of a comparison. There is no arm through which an incomplete
/// observation can become `Match`: the incomplete case is decided before any
/// axis is examined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparison {
    Match,
    Mismatch { axes: Vec<Axis> },
    Incomplete { reason: String },
}

/// Every mode fact in one projection, so a bare permission-bit change is
/// nameable even though it also surfaces as a dirty entry.
fn modes_projection(doc: &WorkspaceDoc) -> Vec<(String, String, String, String)> {
    let mut out: Vec<(String, String, String, String)> = Vec::new();
    for s in &doc.staged {
        out.push((
            s.path.clone(),
            s.head_mode.clone(),
            s.index_mode.clone(),
            String::new(),
        ));
    }
    for d in &doc.dirty {
        out.push((
            d.path.clone(),
            String::new(),
            String::new(),
            d.worktree_mode.clone(),
        ));
    }
    for r in &doc.renamed {
        out.push((
            r.new_path.clone(),
            r.head_mode.clone(),
            r.index_mode.clone(),
            String::new(),
        ));
    }
    out.sort();
    out
}

fn classify(
    expected: &WorkspaceFingerprint,
    observed: &WorkspaceFingerprint,
) -> Result<Comparison> {
    let e = &expected.doc;
    let o = &observed.doc;
    let mut axes = Vec::new();
    if e.repo_id != o.repo_id {
        axes.push(Axis::RepositoryIdentity);
    }
    if e.root != o.root {
        axes.push(Axis::Root);
    }
    if e.git_common_dir != o.git_common_dir || e.worktree_id != o.worktree_id {
        axes.push(Axis::WorktreeIdentity);
    }
    if e.branch != o.branch {
        axes.push(Axis::BranchState);
    }
    if e.head != o.head {
        axes.push(Axis::Head);
    }
    if e.index_digest != o.index_digest {
        axes.push(Axis::Index);
    }
    if e.staged != o.staged {
        axes.push(Axis::Staged);
    }
    if e.dirty != o.dirty {
        axes.push(Axis::DirtyTracked);
    }
    if e.deleted != o.deleted {
        axes.push(Axis::Deleted);
    }
    if e.renamed != o.renamed {
        axes.push(Axis::Renamed);
    }
    if e.conflicts != o.conflicts {
        axes.push(Axis::Conflicts);
    }
    if modes_projection(e) != modes_projection(o) {
        axes.push(Axis::Modes);
    }
    if e.symlinks != o.symlinks {
        axes.push(Axis::Symlinks);
    }
    if e.untracked != o.untracked {
        axes.push(Axis::Untracked);
    }
    if e.submodules != o.submodules {
        axes.push(Axis::Submodules);
    }
    if e.sparse_digest != o.sparse_digest {
        axes.push(Axis::SparseCheckout);
    }
    if e.lfs_digest != o.lfs_digest {
        axes.push(Axis::Lfs);
    }
    if axes.is_empty() {
        if expected.identity != observed.identity {
            return Err(CoreError::Integrity(
                "every axis agrees but the digests differ — the canonical encoder is defective"
                    .to_string(),
            ));
        }
        return Ok(Comparison::Match);
    }
    Ok(Comparison::Mismatch { axes })
}

/// Compare an expected fingerprint against the workspace at `root`.
///
/// The freeze rule, by construction: there is no scope parameter. The
/// observed side is captured under the scope frozen inside `expected` — the
/// scope that was hashed into the expected digest before any mismatch could
/// have been seen. Changing scope means minting a new expectation with a
/// different digest, which is a new, auditable fact — never a quiet retry.
pub fn compare(expected: &WorkspaceFingerprint, root: &Path) -> Result<Comparison> {
    compare_with(expected, root, || {})
}

/// [`compare`] with the observation seam exposed, for the oracle's real
/// mid-capture mutation cases.
pub fn compare_with(
    expected: &WorkspaceFingerprint,
    root: &Path,
    between: impl FnOnce(),
) -> Result<Comparison> {
    match observe_with(root, expected.doc.scope.clone(), between)? {
        Observation::Incomplete { reason, .. } => Ok(Comparison::Incomplete { reason }),
        Observation::Complete(observed) => classify(expected, &observed),
    }
}

/* ── the foreground WORKSPACE_OBSERVED writer ───────────────────────── */

/// What recording produced. Recording abstains — loudly, with the reason —
/// when no repository identity exists, because the envelope's
/// `repository_identity` is NOT NULL and a placeholder would be a lie the
/// chain then preserves forever (§7: unknown stays unknown).
#[derive(Debug)]
pub enum RecordOutcome {
    Recorded {
        outcome: Box<AppendOutcome>,
        workspace_identity: Option<String>,
    },
    NoRepositoryIdentity {
        reason: String,
    },
}

/// Record one WORKSPACE_OBSERVED observation from a foreground command.
///
/// The receipt terminates at independently observed filesystem state: the
/// workspace root, carrying the WSID_V1 canonical digest for a complete
/// observation and no digest for an incomplete one. A stored snapshot row
/// may ride along as a derived reference — context, never the terminal.
/// Foreground callers only; nothing on the hook path writes observations
/// (A205 handoff — that stage inherits the spill discipline first).
pub fn record_observation(
    ledger: &Ledger,
    observation: &Observation,
    observed_at: &str,
    source_session: &str,
    snapshot_row_id: Option<&str>,
) -> Result<RecordOutcome> {
    let (repo_id, root, digest, completeness, incomplete_reason) = match observation {
        Observation::Complete(fp) => (
            fp.doc.repo_id.clone(),
            fp.doc.root.clone(),
            Some(fp.digest_hex().to_string()),
            Completeness::Complete,
            None,
        ),
        Observation::Incomplete {
            reason,
            repo_id,
            root,
        } => (
            repo_id.clone(),
            root.clone(),
            None,
            Completeness::Incomplete,
            Some(reason.clone()),
        ),
    };
    let Some(repository_identity) = repo_id else {
        return Ok(RecordOutcome::NoRepositoryIdentity {
            reason: "no stable repository identity could be derived for this workspace — \
                     the observation is not recorded rather than recorded against a guess"
                .to_string(),
        });
    };
    let mut receipt = SourceReceipt::new(ReceiptTerminal::Filesystem {
        path: root,
        sha256: digest,
    });
    if let Some(id) = snapshot_row_id {
        receipt.derived_refs.push(DerivedRef {
            table: "workspace_snapshots".to_string(),
            id: id.to_string(),
        });
    }
    if let Some(reason) = incomplete_reason {
        receipt
            .extra
            .insert("incomplete_reason".to_string(), reason.into());
    }
    let workspace_identity = match observation {
        Observation::Complete(fp) => Some(fp.identity().to_string()),
        Observation::Incomplete { .. } => None,
    };
    let outcome = append(
        ledger,
        &NewObservation {
            observation_type: ObservationType::WorkspaceObserved,
            source_adapter: "thruline",
            source_session,
            repository_identity: &repository_identity,
            workspace_identity: workspace_identity.as_deref(),
            observed_at,
            completeness,
            receipt: &receipt,
        },
    )?;
    Ok(RecordOutcome::Recorded {
        outcome: Box::new(outcome),
        workspace_identity,
    })
}
