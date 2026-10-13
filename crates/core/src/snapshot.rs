//! C2 — the workspace snapshotter (A171).
//!
//! A read-only capture of what is actually on this machine at a protect
//! boundary: repository lineage, the dirty tree file by file with digests,
//! an environment fingerprint that records names and never values, and a
//! process-tree observation. The capsule (C3) cites these rows; recovery
//! verification (C4) diffs a fresh workspace against them.
//!
//! Three laws:
//!
//! 1. **Read-only, structurally.** The only commands this module runs are
//!    `git status/diff/rev-parse/symbolic-ref/config/--version` and `ps` —
//!    a test walks this file for mutating git verbs and fails on any.
//! 2. **Atomic or honest.** The tree's porcelain state is read before and
//!    after content hashing; if the two reads differ, the snapshot is stored
//!    with `complete = false` and says "Snapshot incomplete — capture again."
//!    A snapshot that silently interleaved a moving tree would be a lie with
//!    checksums on it.
//! 3. **Names, never values.** The environment section records variable
//!    NAMES and lockfile digests. No value crosses into the doc — pinned by
//!    a canary test.
//!
//! **Every child has a time limit** (SNAPSHOT-CHILD-TIMEOUT, D-051). All the
//! children of one capture share one deadline, [`crate::bounded_child::TIME_LIMIT`]
//! from its start. A read that does not finish is never recorded as a value:
//!
//! * a **tree** read — whether this is a worktree, the lineage, the porcelain
//!   state, the dirty diff — that times out means **no snapshot**: `capture`
//!   fails with a timeout. An empty manifest reads as a clean tree, a missing
//!   HEAD as "not a git worktree", and a missing diff digest as "moved", so a
//!   snapshot standing in for an unread tree would carry a claim nobody made;
//! * an **environment or process** read — `git --version`, `ps` — that times
//!   out leaves its field absent (`None`) and is named in the doc's
//!   `timed_out` list. The tree it sits beside was read in full.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// One file in the dirty tree, with the digest of its worktree bytes.
/// Deleted files carry no digest — there are no bytes to speak for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub sha256: Option<String>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TreeManifests {
    pub staged: Vec<FileEntry>,
    pub unstaged: Vec<FileEntry>,
    pub untracked: Vec<FileEntry>,
    pub deleted: Vec<String>,
    /// sha256 of `git diff HEAD` — staged and unstaged together. Untracked
    /// files are outside a diff by definition; their digests are above.
    pub dirty_diff_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitState {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub worktree_root: String,
    pub git_common_dir: String,
    pub remote_origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentFingerprint {
    pub os: String,
    pub arch: String,
    pub git_version: Option<String>,
    /// Sorted names only. Values never cross into the record.
    pub env_names: Vec<String>,
    /// Digests of the dependency lockfiles present at the root.
    pub lockfiles: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessObservations {
    /// The `ps` tree at capture time, verbatim, with its digest.
    pub ps_sha256: Option<String>,
    /// How many lines `ps` printed. `None` when `ps` did not finish within the
    /// time limit — it is then named in [`WorkspaceSnapshot::timed_out`] — so
    /// a list that was never read is not recorded as a count. A stored `n`
    /// reads back as `Some(n)` and writes out as `n`, byte for byte.
    pub ps_lines: Option<u64>,
    /// Named v1 gaps — absent data says so instead of pretending.
    pub ports: Option<String>,
    pub task_ids: Option<String>,
    pub log_cursors: Option<String>,
    pub gaps: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub id: String,
    pub taken_at: String,
    pub root: String,
    /// None when the root is not a git worktree — recorded, not invented.
    pub git: Option<GitState>,
    pub tree: TreeManifests,
    pub environment: EnvironmentFingerprint,
    pub processes: ProcessObservations,
    pub complete: bool,
    pub note: Option<String>,
    /// The reads that did not finish within the capture's time limit, by the
    /// command that was run (`ps -axo pid,ppid,stat,etime,command`,
    /// `git --version`). Each one's field is absent, never a value. Only an
    /// environment or process read can be named here: a tree read that times
    /// out fails the whole capture instead. Skipped when empty, so a capture
    /// in which everything answered stores exactly the doc it always stored,
    /// and like `protected` below it rides inside the content-addressed doc
    /// without a schema change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timed_out: Vec<String>,
    /// A208 (MANDATE-v4 §14): lineage from this digest-only record to the
    /// byte-preserving TLSNAP_V1 layer, when the protect path captured one.
    /// `None` on pre-A208 rows and on captures where the byte layer refused
    /// (stated on the CLI at the moment, never silently). Rides inside the
    /// content-addressed doc, so it is hash-covered without a schema change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected: Option<crate::protected_store::ProtectedRef>,
}

/// The programs a capture runs and the time it may take. The public capture
/// uses the system's `git` and `ps` from `PATH` and
/// [`crate::bounded_child::TIME_LIMIT`]; the tests put stand-ins in front, the
/// same kind of seam as `between`.
pub(crate) struct Tools {
    pub(crate) git: OsString,
    pub(crate) ps: OsString,
    pub(crate) limit: Duration,
}

impl Tools {
    pub(crate) fn system() -> Tools {
        Tools {
            git: OsString::from("git"),
            ps: OsString::from("ps"),
            limit: crate::bounded_child::TIME_LIMIT,
        }
    }
}

/// What one read produced. `NoAnswer` is a child that ran and failed, or that
/// could not be started: the answer that existed before this card. `TimedOut`
/// is new, and the two are never merged. Shutdown interruption aborts capture
/// rather than becoming a failed read, an empty tree, or a zero process count.
enum Read {
    Answer(String),
    NoAnswer,
    TimedOut,
    Interrupted(std::io::Error),
}

fn git_command(tools: &Tools, root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(&tools.git);
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .arg("-C")
        .arg(root)
        .args(args);
    cmd
}

fn git_read(tools: &Tools, root: &Path, args: &[&str], deadline: Instant) -> Read {
    match crate::bounded_child::output_until(&mut git_command(tools, root, args), deadline) {
        Ok(crate::bounded_child::Outcome::Finished(out)) if out.status.success() => {
            Read::Answer(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
        }
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Read::Interrupted(error),
        Ok(crate::bounded_child::Outcome::Finished(_)) | Err(_) => Read::NoAnswer,
        Ok(crate::bounded_child::Outcome::TimedOut) => Read::TimedOut,
    }
}

/// A read outside a capture, with the time limit to itself. A timeout is
/// `None`, which every caller already reads as unknown.
fn git_out(root: &Path, args: &[&str]) -> Option<String> {
    let tools = Tools::system();
    match git_read(&tools, root, args, Instant::now() + tools.limit) {
        Read::Answer(text) => Some(text),
        Read::NoAnswer | Read::TimedOut | Read::Interrupted(_) => None,
    }
}

/// A read the tree depends on. A timeout is an error: no snapshot is better
/// than one that records an unread tree as a value.
fn tree_read(
    tools: &Tools,
    root: &Path,
    args: &[&str],
    deadline: Instant,
) -> Result<Option<String>> {
    match git_read(tools, root, args, deadline) {
        Read::Answer(text) => Ok(Some(text)),
        Read::NoAnswer => Ok(None),
        Read::Interrupted(error) => Err(CoreError::io(root, error)),
        Read::TimedOut => Err(crate::bounded_child::timed_out(
            &format!("git {}", args.join(" ")),
            tools.limit,
        )),
    }
}

/// Run one tree command to completion within the deadline. A timeout is an
/// error, for the reason [`tree_read`] gives.
fn tree_output(
    tools: &Tools,
    root: &Path,
    args: &[&str],
    deadline: Instant,
) -> Result<std::process::Output> {
    match crate::bounded_child::output_until(&mut git_command(tools, root, args), deadline)
        .map_err(|e| CoreError::io(root, e))?
    {
        crate::bounded_child::Outcome::Finished(out) => Ok(out),
        crate::bounded_child::Outcome::TimedOut => Err(crate::bounded_child::timed_out(
            &format!("git {}", args.join(" ")),
            tools.limit,
        )),
    }
}

/// Parse `git status --porcelain -z` into (XY, path) pairs.
fn porcelain(tools: &Tools, root: &Path, deadline: Instant) -> Result<Vec<(String, String)>> {
    let out = tree_output(tools, root, &["status", "--porcelain", "-z"], deadline)?;
    if !out.status.success() {
        return Err(CoreError::Config(format!(
            "git status failed at {}",
            root.display()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut entries = Vec::new();
    let mut parts = text.split('\0').filter(|s| !s.is_empty()).peekable();
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let xy = entry[..2].to_string();
        let path = entry[3..].to_string();
        // Renames carry the original path as the next NUL field; the record
        // keeps the current name, which is what a digest can speak for.
        if xy.starts_with('R') || xy.starts_with('C') {
            let _original = parts.next();
        }
        entries.push((xy, path));
    }
    Ok(entries)
}

fn digest_file(root: &Path, rel: &str) -> FileEntry {
    let path = root.join(rel);
    match std::fs::read(&path) {
        Ok(bytes) => FileEntry {
            path: rel.to_string(),
            sha256: Some(sha256_hex(&bytes)),
            bytes: Some(bytes.len() as u64),
        },
        Err(_) => FileEntry {
            path: rel.to_string(),
            sha256: None,
            bytes: None,
        },
    }
}

const LOCKFILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "uv.lock",
    "poetry.lock",
    "Gemfile.lock",
    "go.sum",
];

fn environment(
    tools: &Tools,
    root: &Path,
    deadline: Instant,
    timed_out: &mut Vec<String>,
) -> Result<EnvironmentFingerprint> {
    let mut env_names: Vec<String> = std::env::vars_os()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    env_names.sort();
    let mut lockfiles = BTreeMap::new();
    for name in LOCKFILES {
        if let Ok(bytes) = std::fs::read(root.join(name)) {
            lockfiles.insert(name.to_string(), sha256_hex(&bytes));
        }
    }
    let git_version = match git_read(tools, root, &["--version"], deadline) {
        Read::Answer(text) => Some(text),
        Read::NoAnswer => None,
        Read::Interrupted(error) => return Err(CoreError::io(root, error)),
        Read::TimedOut => {
            timed_out.push("git --version".to_string());
            None
        }
    };
    Ok(EnvironmentFingerprint {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        git_version,
        env_names,
        lockfiles,
    })
}

const PS_ARGS: [&str; 2] = ["-axo", "pid,ppid,stat,etime,command"];

fn processes(
    tools: &Tools,
    deadline: Instant,
    timed_out: &mut Vec<String>,
) -> Result<ProcessObservations> {
    let ps = crate::bounded_child::output_until(Command::new(&tools.ps).args(PS_ARGS), deadline);
    let (ps_sha256, ps_lines) = match ps {
        Ok(crate::bounded_child::Outcome::Finished(out)) if out.status.success() => (
            Some(sha256_hex(&out.stdout)),
            Some(String::from_utf8_lossy(&out.stdout).lines().count() as u64),
        ),
        Ok(crate::bounded_child::Outcome::TimedOut) => {
            timed_out.push(format!("ps {}", PS_ARGS.join(" ")));
            (None, None)
        }
        // A `ps` that ran and failed is recorded as it was before this card:
        // no digest and a count of 0. That ambiguity predates the time limit
        // and is left as it was, so a normal run stays byte-identical.
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
            return Err(CoreError::io("ps", error))
        }
        Ok(crate::bounded_child::Outcome::Finished(_)) | Err(_) => (None, Some(0)),
    };
    Ok(ProcessObservations {
        ps_sha256,
        ps_lines,
        ports: None,
        task_ids: None,
        log_cursors: None,
        gaps: "ports, task ids, and log cursors are not captured in v1 — named gaps, \
               not silent ones"
            .to_string(),
    })
}

/// Capture a snapshot of `root`. `between` runs after the first tree read and
/// before the second — a test seam for the atomicity law; the public caller
/// passes a no-op.
pub fn capture_with(
    root: &Path,
    taken_at: &str,
    between: impl FnOnce(),
) -> Result<WorkspaceSnapshot> {
    capture_using(&Tools::system(), root, taken_at, between)
}

/// [`capture_with`] with the programs and the limit named.
pub(crate) fn capture_using(
    tools: &Tools,
    root: &Path,
    taken_at: &str,
    between: impl FnOnce(),
) -> Result<WorkspaceSnapshot> {
    let deadline = Instant::now() + tools.limit;
    let root_txt = root.display().to_string();
    let is_repo = tree_read(
        tools,
        root,
        &["rev-parse", "--is-inside-work-tree"],
        deadline,
    )?
    .is_some_and(|s| s == "true");

    let (git, tree, complete, note) = if is_repo {
        let read = |args: &[&str]| tree_read(tools, root, args, deadline);
        let git = GitState {
            branch: read(&["symbolic-ref", "--short", "-q", "HEAD"])?,
            head: read(&["rev-parse", "HEAD"])?,
            worktree_root: read(&["rev-parse", "--show-toplevel"])?
                .unwrap_or_else(|| root_txt.clone()),
            git_common_dir: read(&["rev-parse", "--git-common-dir"])?.unwrap_or_default(),
            remote_origin: read(&["config", "--get", "remote.origin.url"])?,
        };
        let first = porcelain(tools, root, deadline)?;
        between();
        let mut tree = TreeManifests::default();
        for (xy, path) in &first {
            let (index, worktree) = (xy.as_bytes()[0] as char, xy.as_bytes()[1] as char);
            if xy == "??" {
                tree.untracked.push(digest_file(root, path));
                continue;
            }
            if index == 'D' || worktree == 'D' {
                tree.deleted.push(path.clone());
                continue;
            }
            if index != ' ' && index != '?' {
                tree.staged.push(digest_file(root, path));
            }
            if worktree != ' ' && worktree != '?' {
                tree.unstaged.push(digest_file(root, path));
            }
        }
        let diff = tree_output(tools, root, &["diff", "HEAD"], deadline)?;
        tree.dirty_diff_sha256 = diff.status.success().then(|| sha256_hex(&diff.stdout));
        let second = porcelain(tools, root, deadline)?;
        if first == second {
            (Some(git), tree, true, None)
        } else {
            (
                Some(git),
                tree,
                false,
                Some(
                    "Snapshot incomplete — capture again. The tree moved mid-capture.".to_string(),
                ),
            )
        }
    } else {
        (
            None,
            TreeManifests::default(),
            true,
            Some("not a git worktree — no tree manifests to record".to_string()),
        )
    };

    let mut timed_out = Vec::new();
    let environment = environment(tools, root, deadline, &mut timed_out)?;
    let processes = processes(tools, deadline, &mut timed_out)?;
    Ok(WorkspaceSnapshot {
        id: Ulid::generate().to_string(),
        taken_at: taken_at.to_string(),
        root: root_txt,
        git,
        tree,
        environment,
        processes,
        complete,
        note,
        timed_out,
        protected: None,
    })
}

/// The public capture: no seam.
pub fn capture(root: &Path, taken_at: &str) -> Result<WorkspaceSnapshot> {
    capture_with(root, taken_at, || {})
}

/// Store a snapshot as an append-only receipt. Returns the doc digest.
pub fn store(ledger: &Ledger, snapshot: &WorkspaceSnapshot) -> Result<String> {
    let doc = serde_json::to_string_pretty(snapshot)?;
    let doc_sha256 = sha256_hex(doc.as_bytes());
    ledger.conn().execute(
        "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete, doc, doc_sha256)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        rusqlite::params![
            snapshot.id,
            snapshot.taken_at,
            snapshot.root,
            snapshot.git.as_ref().and_then(|g| g.head.clone()),
            snapshot.git.as_ref().and_then(|g| g.branch.clone()),
            snapshot.complete,
            doc,
            doc_sha256,
        ],
    )?;
    Ok(doc_sha256)
}

/// The latest stored snapshot for `root` that carries a byte layer — the
/// lineage anchor for the next protected capture of the same workspace.
/// A cursor over recent rows, newest first, stopping at the first hit.
pub fn latest_protected(
    ledger: &Ledger,
    root: &str,
) -> Result<Option<(String, crate::protected_store::ProtectedRef)>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT id, doc, doc_sha256 FROM workspace_snapshots
          WHERE root = ?1 ORDER BY rowid DESC",
    )?;
    let rows = stmt.query_map([root], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (id, doc, stored_sha) = row?;
        if sha256_hex(doc.as_bytes()) != stored_sha {
            return Err(CoreError::Integrity(format!(
                "workspace snapshot {id} does not match its recorded digest"
            )));
        }
        let snapshot: WorkspaceSnapshot = serde_json::from_str(&doc)?;
        if let Some(protected) = snapshot.protected {
            return Ok(Some((id, protected)));
        }
    }
    Ok(None)
}

/// Load a stored snapshot doc by id, with its digest re-checked.
pub fn load(ledger: &Ledger, id: &str) -> Result<Option<(WorkspaceSnapshot, String)>> {
    use rusqlite::OptionalExtension;
    let row: Option<(String, String)> = ledger
        .conn()
        .query_row(
            "SELECT doc, doc_sha256 FROM workspace_snapshots WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((doc, stored_sha)) = row else {
        return Ok(None);
    };
    let sha = sha256_hex(doc.as_bytes());
    if sha != stored_sha {
        return Err(CoreError::Integrity(format!(
            "workspace snapshot {id} does not match its recorded digest"
        )));
    }
    Ok(Some((serde_json::from_str(&doc)?, stored_sha)))
}

/// The repository-relative paths whose **committed** content differs between
/// two commits, as git itself reports them.
///
/// T-F-1-F5. [`TreeManifests`] lists only the dirty working tree — staged,
/// unstaged, untracked, deleted — because that is what a snapshot is for.
/// Once an edit is committed the working tree is clean again and the edit
/// leaves no trace in any manifest, so two snapshots taken either side of a
/// commit compare as identical. This is the one read that can see across
/// that boundary. Read-only: `diff`, nothing else, and no working tree is
/// touched.
///
/// `None` when git cannot answer — either commit unknown here, not a
/// repository, git missing. Unknown is returned as unknown; the caller must
/// not read it as "nothing changed".
pub fn committed_changed_paths(root: &Path, old_head: &str, new_head: &str) -> Option<Vec<String>> {
    if old_head == new_head {
        return Some(Vec::new());
    }
    let out = git_out(root, &["diff", "--name-only", "-z", old_head, new_head])?;
    Some(
        out.split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    /// A stand-in for a program: when its arguments contain `stall_on` it
    /// starts a child and waits on it for 60 s; otherwise it hands the call to
    /// `real`. Written inside the test's own temporary directory.
    fn stand_in(dir: &Path, name: &str, real: &str, stall_on: &str) -> OsString {
        let path = dir.join(name);
        let script = format!(
            "#!/bin/sh\ncase \" $* \" in\n  *\" {stall_on} \"*) sleep 60 & wait ;;\nesac\nexec {real} \"$@\"\n"
        );
        std::fs::write(&path, script).expect("stand-in");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path.into_os_string()
    }

    fn tools(git: OsString, ps: OsString) -> Tools {
        Tools {
            git,
            ps,
            limit: Duration::from_secs(5),
        }
    }

    fn repo(dir: &Path) -> std::path::PathBuf {
        let root = dir.join("repo");
        std::fs::create_dir_all(&root).expect("repo dir");
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .expect("git init");
        assert!(init.success());
        std::fs::write(root.join("a.txt"), "a").expect("file");
        root
    }

    /// `git` does not say whether the folder is a worktree. The capture fails
    /// with a timeout, promptly, instead of recording "not a git worktree".
    #[test]
    fn a_git_that_does_not_answer_is_no_snapshot_rather_than_not_a_worktree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let git = stand_in(dir.path(), "git", "git", "--is-inside-work-tree");
        let started = Instant::now();
        let result = capture_using(
            &tools(git, OsString::from("ps")),
            dir.path(),
            "2026-09-27T00:00:00Z",
            || {},
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        match result {
            Err(err) => assert!(crate::bounded_child::is_timeout(&err), "{err}"),
            Ok(snap) => panic!("an unread folder was recorded: {:?}", snap.note),
        }
    }

    /// Inside a repository, the porcelain read stalls. No snapshot: its empty
    /// manifests would read as a clean tree.
    #[test]
    fn a_tree_read_that_does_not_answer_is_no_snapshot_rather_than_a_clean_tree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(dir.path());
        let git = stand_in(dir.path(), "git", "git", "--porcelain");
        let result = capture_using(
            &tools(git, OsString::from("ps")),
            &root,
            "2026-09-27T00:00:00Z",
            || {},
        );
        match result {
            Err(err) => assert!(crate::bounded_child::is_timeout(&err), "{err}"),
            Ok(snap) => panic!("an unread tree was recorded: {:?}", snap.tree),
        }
    }

    /// `ps` does not answer. The snapshot is taken, the process list is absent
    /// — no digest and no count — and the read is named.
    #[test]
    fn a_ps_that_does_not_answer_is_named_and_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(dir.path());
        let ps = stand_in(dir.path(), "ps", "ps", "-axo");
        let snap = capture_using(
            &tools(OsString::from("git"), ps),
            &root,
            "2026-09-27T00:00:00Z",
            || {},
        )
        .expect("the tree was read");
        assert!(snap.git.is_some(), "the tree read is unaffected");
        assert_eq!(snap.processes.ps_sha256, None);
        assert_eq!(snap.processes.ps_lines, None);
        assert_eq!(snap.timed_out, vec!["ps -axo pid,ppid,stat,etime,command"]);
        let doc: serde_json::Value = serde_json::to_value(&snap).expect("json");
        assert!(doc["processes"]["ps_lines"].is_null(), "{doc}");
        assert_eq!(doc["timed_out"][0], "ps -axo pid,ppid,stat,etime,command");
    }

    /// Nothing stalls. The doc is the one a capture always stored: a count and
    /// a digest for the process list, and no `timed_out` key.
    #[test]
    fn a_capture_in_which_everything_answers_stores_the_doc_it_always_did() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(dir.path());
        let snap = capture(&root, "2026-09-27T00:00:00Z").expect("capture");
        assert!(snap.timed_out.is_empty());
        let doc: serde_json::Value = serde_json::to_value(&snap).expect("json");
        assert!(doc.get("timed_out").is_none(), "{doc}");
        assert!(doc["processes"]["ps_lines"].is_u64(), "{doc}");
        assert!(doc["processes"]["ps_sha256"].is_string(), "{doc}");
        assert!(doc["environment"]["git_version"].is_string(), "{doc}");
    }
}
