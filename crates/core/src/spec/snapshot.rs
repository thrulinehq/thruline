//! The §4.4 snapshot engine: a git tree hash of the working directory,
//! including untracked files, taken without touching the user's index.
//!
//! ```text
//! Snapshot algorithm: `git add -A` into a **temporary index file**
//! (`GIT_INDEX_FILE`), `git write-tree`, discard index. Never touch the
//! user's real index. Respect `.gitignore` plus `.thrulineignore`.
//! ```
//!
//! # The dangerous part, and how it is contained
//!
//! `git add -A` is a mutating command. Run against the real index it would
//! stage the user's entire working tree — silently changing what their next
//! `git commit` captures. Everything below exists to make that impossible
//! rather than merely unlikely:
//!
//! - `GIT_INDEX_FILE` is set to a path inside a freshly created temporary
//!   directory, and it is **absolute**. A relative path would be resolved
//!   against the repository root by git, not against our process's cwd, and
//!   `.git/index` is exactly the kind of name that resolution can land on.
//! - The temporary directory is a [`tempfile::TempDir`] owned by the call, so
//!   the index is removed on every exit path including a panic.
//! - `snapshot_leaves_the_users_index_byte_identical` in
//!   `crates/core/tests/snapshot_engine.rs` reads the real index before and
//!   after and compares bytes and mtime. It is the test that matters most in
//!   this module.
//!
//! # What it *does* touch
//!
//! Stated because it is real and a reader deserves to know: `git add` writes
//! blob and tree objects into the repository's object database. A snapshot of
//! a dirty tree therefore leaves loose objects in `.git/objects` that nothing
//! references until a later `git gc` prunes them. This is inherent to
//! `write-tree` — a tree hash that named content git had never stored would be
//! a hash of nothing, and §7 re-execution has to materialise that content — and
//! it is not destructive: no ref moves, no existing object changes, and
//! `git gc --prune=now` collects them as ordinary unreachable objects.
//!
//! **The cost, measured rather than characterised** (BUILD-PLAN C.6's condition
//! for accepting it). On a 30-file fixture repository: 50 snapshots, each taken
//! after changing one file, added **151 objects / 604 KB**; 20 further snapshots
//! with **no** change added **0**. Objects are content-addressed, so growth is
//! bounded by the distinct content the agent actually wrote, not by how often
//! ThruLine snapshots — `snapshotting_an_unchanged_tree_writes_no_new_git_objects`
//! keeps that true. `thruline doctor` owes the user the live equivalent of the
//! first figure and the reclaim command; `thruline init` owes them the sentence
//! before it starts. Learning this from `du` would be a trust failure of exactly
//! the kind this product is about.
//!
//! # `.thrulineignore`
//!
//! Git has no "additional excludes file" flag; `core.excludesFile` is a single
//! path. Overriding it for our invocation would *drop* the user's own global
//! excludes and pull files into the snapshot that they have told git to ignore
//! everywhere. So when `.thrulineignore` exists, this module reads the user's
//! configured excludes file, concatenates it with `.thrulineignore` into a
//! temporary file, and points `core.excludesFile` at that — for one process,
//! via `-c`, never written to any config. When `.thrulineignore` is absent,
//! `-c` is not passed at all and the user's configuration is untouched.
//!
//! # Content filters break tree binding, and this module must not be silent
//!
//! BUILD-SPEC §0 principle 3 is "bind everything to a tree hash". A
//! `.gitattributes` **clean filter** breaks that binding at its root: git runs
//! the filter on the way into the object database, so the tree names the
//! *filtered* bytes while the working tree holds the originals.
//!
//! This was demonstrated against this engine, not theorised. An agent adding
//! `PYTEST_CURRENT_TEST` special-casing below a marker that a clean filter
//! strips changes the worktree bytes and leaves the tree hash identical. The
//! consequence is worst exactly where the evidence is meant to be strongest:
//! §7 re-execution materialises the **honest** code from the clean tree, runs
//! it, and stamps the false claim `reexecuted: true`. A clean filter turns
//! re-execution into a laundering machine.
//!
//! This module cannot fix that — the fix is a policy decision above it — but it
//! must never hide it. Every snapshot therefore reports
//! [`TreeSnapshot::content_filters`], and a snapshot taken where a `filter=`
//! driver is in scope is `degrades_capture()`. §13 says degraded capture makes
//! claims `unverifiable`, never `supported`, and that is the rule this field
//! exists to feed.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A point-in-time tree hash of the working directory (§3.1 `snapshot`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeSnapshot {
    /// `git write-tree` output: the tree object naming the whole working tree,
    /// untracked non-ignored files included.
    pub tree_hash: String,
    /// The commit `HEAD` resolves to, or `None` in a repository with no
    /// commits yet.
    pub head_commit: Option<String>,
    /// Whether the working tree differs from `HEAD`.
    pub dirty: bool,
    /// How many paths differ between `HEAD`'s tree and this snapshot. In a
    /// repository with no commits, every file in the tree.
    pub changed_paths_count: u64,
    /// Content transforms in scope that can make `tree_hash` disagree with the
    /// bytes on disk. See the module docs.
    pub content_filters: ContentFilters,
    /// Working-tree entries git cannot put in a tree, and therefore silently
    /// left out. See [`OmittedEntry`].
    pub omitted: Vec<OmittedEntry>,
}

/// A working-tree entry that is not in the snapshot because git cannot
/// represent it.
///
/// **Found by measurement, not by reading.** A FIFO in the working directory is
/// omitted by `git add -A` with **exit 0, no warning, and no diagnostic** — and
/// `git status --porcelain` does not mention it either, so the resulting tree is
/// byte-identical to one taken with no FIFO present. Sockets and device nodes
/// behave the same way.
///
/// This is not evidence loss in the sense a clean filter is: a pipe holds no
/// persistent content, so there is nothing a tree hash could have named. It is
/// reported rather than ignored because "the tree hash names the whole working
/// directory" would otherwise be a claim with a silent exception, and silent
/// omission is the failure mode this engine already rejected once, for nested
/// repositories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmittedEntry {
    /// Path relative to the repository root.
    pub path: String,
    /// `fifo`, `socket`, `block-device`, `char-device`, or `unknown`.
    pub kind: &'static str,
}

/// `.gitattributes` transforms in scope for a snapshot.
///
/// Reported on every snapshot rather than looked up on demand, because the
/// question "was this tree hash a faithful name for the working tree?" has to
/// be answerable later, from the record, by someone who no longer has the
/// repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentFilters {
    /// Filter drivers named by a `filter=<name>` attribute. These run arbitrary
    /// user commands over file contents and can hide anything.
    pub drivers: std::collections::BTreeSet<String>,
    /// Other content-altering attributes seen: `ident` (substitutes `$Id$`),
    /// `text` / `eol` (line-ending normalisation).
    pub other_transforms: std::collections::BTreeSet<String>,
    /// Which attributes files declared them, for the message a human reads.
    pub sources: Vec<String>,
}

impl ContentFilters {
    /// Whether a snapshot taken under these filters may be trusted to name the
    /// working tree's actual bytes.
    ///
    /// `filter=` is arbitrary code and degrades capture. `ident` rewrites
    /// content and degrades it too. `text`/`eol` normalise line endings — real,
    /// worth recording, and **not** degrading: they cannot hide a changed
    /// statement, only a changed line terminator, and treating every
    /// `* text=auto` repository as degraded would make the flag meaningless in
    /// the repositories that have one.
    pub fn degrades_capture(&self) -> bool {
        !self.drivers.is_empty() || self.other_transforms.contains("ident")
    }

    /// One line for a human, naming what was found and where.
    pub fn describe(&self) -> String {
        if self.drivers.is_empty() && self.other_transforms.is_empty() {
            return "no content filters in scope".to_string();
        }
        let mut parts = Vec::new();
        if !self.drivers.is_empty() {
            parts.push(format!(
                "filter drivers [{}]",
                self.drivers.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if !self.other_transforms.is_empty() {
            parts.push(format!(
                "attributes [{}]",
                self.other_transforms
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        format!(
            "{} declared in {}",
            parts.join(" and "),
            self.sources.join(", ")
        )
    }
}

/// A snapshot could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// The path is not inside a git repository, or `git` is not installed.
    #[error("{path} is not a usable git repository: {reason}")]
    NotARepository { path: PathBuf, reason: String },
    /// A git invocation failed.
    #[error("`git {argv}` failed with {status}: {stderr}")]
    Git {
        argv: String,
        status: String,
        stderr: String,
    },
    /// Git could not be run at all.
    #[error("could not run `git {argv}`")]
    Spawn {
        argv: String,
        #[source]
        source: std::io::Error,
    },
    /// The temporary index could not be created.
    #[error("could not create the temporary index directory")]
    TempDir(#[source] std::io::Error),
    /// Git printed something that is not a hash where a hash was required.
    #[error("`git {argv}` printed {output:?}, which is not an object id")]
    NotAnObjectId { argv: String, output: String },
    /// A nested repository with no commit checked out.
    ///
    /// `git add -A` refuses outright — it cannot record a gitlink for a
    /// submodule-shaped directory that has no commit — so **there is no tree**
    /// and nothing downstream can bind to one. Given its own variant because
    /// the generic `Git` error buries the actionable part, and because the
    /// only safe handling is to fail loudly: a snapshot that quietly skipped
    /// the nested directory would produce a tree hash that silently omits part
    /// of the working tree, which is worse than no tree at all.
    #[error(
        "a nested git repository at {path} has no commit checked out, so `git add` \
         cannot record it and no tree exists for this working directory. Commit \
         inside it, remove it, or add it to .thrulineignore."
    )]
    NestedRepositoryWithoutCommit { path: String },
}

/// Recognise git's refusal to add a commitless nested repository.
///
/// Matched on the stable part of the message. If git ever rewords it the match
/// fails *open* — back to the generic `Git` error, which is still loud and
/// still carries the stderr verbatim, so this can never turn into a silent
/// skip. `nested_repository_message_is_recognised` pins the current wording
/// against a real git.
fn nested_repository_path(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        if let Some(idx) = line.find(" does not have a commit checked out") {
            let head = &line[..idx];
            let path = head
                .rsplit_once('\'')
                .and_then(|(before, _)| before.rsplit_once('\'').map(|(_, p)| p.to_string()))
                .or_else(|| head.split_whitespace().last().map(str::to_string))?;
            return Some(path.trim_matches('\'').to_string());
        }
    }
    None
}

/// The extra ignore file this module honours alongside `.gitignore`.
pub const THRULINE_IGNORE: &str = ".thrulineignore";

/// Take a snapshot of `repo_root`'s working tree.
///
/// Leaves the user's index, refs, config and working tree unchanged. See the
/// module docs for the one thing it does write (unreferenced objects).
pub fn snapshot_tree(repo_root: &Path) -> Result<TreeSnapshot, SnapshotError> {
    let root = resolve_repo_root(repo_root)?;

    let scratch = tempfile::tempdir().map_err(SnapshotError::TempDir)?;
    // Absolute, and outside the repository. Git resolves a relative
    // GIT_INDEX_FILE against the repository root, so a relative path here
    // could name the real index.
    let index_path = scratch.path().join("thruline-snapshot.index");
    debug_assert!(index_path.is_absolute());

    let excludes = combined_excludes_file(&root, scratch.path())?;

    // Stage everything into the temporary index. `-A` includes untracked and
    // deleted paths; `.gitignore` (and, through `excludes`, `.thrulineignore`)
    // still apply, because `-A` is not `-f`.
    if let Err(error) = run_git(
        &root,
        &["add", "-A", "--", "."],
        Some(&index_path),
        &excludes,
    ) {
        if let SnapshotError::Git { stderr, .. } = &error {
            if let Some(path) = nested_repository_path(stderr) {
                return Err(SnapshotError::NestedRepositoryWithoutCommit { path });
            }
        }
        return Err(error);
    }
    let tree_hash = run_git(&root, &["write-tree"], Some(&index_path), &excludes)?;
    let tree_hash = object_id(&tree_hash, "write-tree")?;

    // The user's index is not consulted for anything below, and none of these
    // commands write.
    let head_commit = match run_git(&root, &["rev-parse", "HEAD"], None, &[]) {
        Ok(out) => Some(object_id(&out, "rev-parse HEAD")?),
        // An unborn HEAD is a normal state, not a failure.
        Err(SnapshotError::Git { .. }) => None,
        Err(other) => return Err(other),
    };

    let changed_paths_count = match &head_commit {
        Some(commit) => {
            let out = run_git(
                &root,
                &[
                    "diff-tree",
                    "-r",
                    "--name-only",
                    "--no-commit-id",
                    &format!("{commit}^{{tree}}"),
                    &tree_hash,
                ],
                None,
                &[],
            )?;
            count_lines(&out)
        }
        // No commit to compare against: every path in the snapshot is new.
        None => count_lines(&run_git(
            &root,
            &["ls-tree", "-r", "--name-only", &tree_hash],
            None,
            &[],
        )?),
    };

    let omitted = omitted_entries(&root)?;
    if !omitted.is_empty() {
        tracing::warn!(
            count = omitted.len(),
            "working-tree entries git cannot represent were left out of the snapshot"
        );
    }

    let content_filters = content_filters_in_scope(&root, &tree_hash)?;
    if content_filters.degrades_capture() {
        tracing::warn!(
            tree_hash = %tree_hash,
            filters = %content_filters.describe(),
            "snapshot taken under content filters: the tree hash may not name the \
             bytes on disk, and claims bound to it cannot reach `supported`"
        );
    }

    Ok(TreeSnapshot {
        tree_hash,
        head_commit,
        dirty: changed_paths_count > 0,
        changed_paths_count,
        content_filters,
        omitted,
    })
}

/// Working-tree entries git cannot put in a tree.
///
/// Walks the tree once for non-regular entries — normally none, in which case
/// nothing else runs — and only then asks git, in a single `check-ignore`, which
/// of them the user had already excluded. An ignored FIFO is not an omission;
/// it is a file the user told us to skip.
fn omitted_entries(root: &Path) -> Result<Vec<OmittedEntry>, SnapshotError> {
    let mut found: Vec<OmittedEntry> = Vec::new();
    collect_non_regular(root, root, &mut found);
    if found.is_empty() {
        return Ok(found);
    }

    let mut child = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(root)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env_remove("GIT_INDEX_FILE")
        .args(["check-ignore", "--stdin", "-z"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|source| SnapshotError::Spawn {
            argv: "check-ignore --stdin -z".to_string(),
            source,
        })?;
    let requests: Vec<u8> = found
        .iter()
        .flat_map(|e| {
            let mut bytes = e.path.clone().into_bytes();
            bytes.push(0);
            bytes
        })
        .collect();
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(&requests);
        });
    }
    // `check-ignore` exits 1 when nothing matched, which is a normal answer and
    // not a failure — so the status is not consulted, only the output.
    let output = child
        .wait_with_output()
        .map_err(|source| SnapshotError::Spawn {
            argv: "check-ignore --stdin -z".to_string(),
            source,
        })?;
    let ignored: std::collections::BTreeSet<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    found.retain(|entry| !ignored.contains(&entry.path));
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

fn collect_non_regular(root: &Path, dir: &Path, out: &mut Vec<OmittedEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `.git` is git's own business and is never part of a tree.
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        let Ok(meta) = entry.file_type() else {
            continue;
        };
        if meta.is_dir() {
            collect_non_regular(root, &path, out);
        } else if !meta.is_file() && !meta.is_symlink() {
            let kind = file_kind(&meta);
            if let Ok(relative) = path.strip_prefix(root) {
                out.push(OmittedEntry {
                    path: relative.to_string_lossy().to_string(),
                    kind,
                });
            }
        }
    }
}

#[cfg(unix)]
fn file_kind(meta: &std::fs::FileType) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
    if meta.is_fifo() {
        "fifo"
    } else if meta.is_socket() {
        "socket"
    } else if meta.is_block_device() {
        "block-device"
    } else if meta.is_char_device() {
        "char-device"
    } else {
        "unknown"
    }
}

#[cfg(not(unix))]
fn file_kind(_meta: &std::fs::FileType) -> &'static str {
    "unknown"
}

/// Every `.gitattributes` source in scope, scanned for content transforms.
///
/// Three sources, all of them real:
///
/// - `.gitattributes` files **inside the snapshot tree** — which, because the
///   tree includes untracked files, covers one an agent just wrote and has not
///   committed. That is the case that matters: an agent installing a laundering
///   filter would not commit it first.
/// - `.git/info/attributes`, which is per-clone and never committed.
/// - `core.attributesFile`, the user's global one.
///
/// Read from the tree via `cat-file` rather than from disk, so the scan sees
/// exactly the attributes that applied to the snapshot it is describing.
fn content_filters_in_scope(root: &Path, tree_hash: &str) -> Result<ContentFilters, SnapshotError> {
    let mut found = ContentFilters::default();

    // `-z` and `core.quotePath=false` for two independent reasons:
    //
    //  - by default git C-quotes non-ASCII paths (`"dir\303\251/x"`), so a
    //    `.gitattributes` under an accented directory would arrive with a
    //    trailing `"` and the suffix test below would miss it — and a file
    //    hiding a laundering filter is exactly the file worth putting there;
    //  - a path may legally contain a newline, so line splitting is unsound.
    //
    // The long form (`ls-tree -r -z` without `--name-only`) is used so the blob
    // oid comes back with the path and the contents can be fetched in ONE
    // `cat-file --batch`. The first version spawned one `cat-file` per
    // attributes file: a monorepo with hundreds of them turned every snapshot
    // into hundreds of process spawns, against a 50 ms hook budget.
    let listing = run_git(
        root,
        &["ls-tree", "-r", "-z", tree_hash],
        None,
        &["-c".to_string(), "core.quotePath=false".to_string()],
    )?;

    let mut wanted: Vec<(String, String)> = Vec::new();
    for entry in listing.split('\0').filter(|e| !e.is_empty()) {
        // `<mode> SP <type> SP <oid> TAB <path>`
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        if path != ".gitattributes" && !path.ends_with("/.gitattributes") {
            continue;
        }
        let mut fields = meta.split_whitespace();
        let (_mode, kind, oid) = match (fields.next(), fields.next(), fields.next()) {
            (Some(m), Some(k), Some(o)) => (m, k, o),
            _ => continue,
        };
        // A `.gitattributes` that is a submodule or a symlink is not a file we
        // can read as attributes; skipping it is correct, and skipping it
        // SILENTLY is not, so it is recorded as an unreadable source.
        if kind != "blob" {
            found.sources.push(format!("({path}: {kind}, not read)"));
            continue;
        }
        wanted.push((oid.to_string(), path.to_string()));
    }

    for (blob, (_, path)) in batch_cat_file(root, &wanted)?.into_iter().zip(&wanted) {
        scan_attributes(&blob, path, &mut found);
    }

    if let Ok(text) = std::fs::read_to_string(root.join(".git/info/attributes")) {
        scan_attributes(&text, "(.git/info/attributes)", &mut found);
    }
    if let Ok(configured) = run_git(root, &["config", "--get", "core.attributesFile"], None, &[]) {
        let configured = expand_tilde(configured.trim());
        if let Ok(text) = std::fs::read_to_string(&configured) {
            scan_attributes(
                &text,
                &format!("(core.attributesFile {})", configured.display()),
                &mut found,
            );
        }
    }

    found.sources.sort();
    found.sources.dedup();
    Ok(found)
}

/// Read many blobs with a single `git cat-file --batch`.
///
/// Returns one entry per requested oid, in order. A missing or unreadable blob
/// yields an empty string rather than an error: a `.gitattributes` we cannot
/// read declares no filters we can see, and the caller has already recorded
/// that the source exists.
///
/// stdin is written from a separate thread. Writing the whole request list
/// inline and only then reading would deadlock once the requests exceed the
/// pipe buffer — around 1,500 attributes files on a 64 KB pipe — which is the
/// exact monorepo this batching exists to serve.
fn batch_cat_file(root: &Path, wanted: &[(String, String)]) -> Result<Vec<String>, SnapshotError> {
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let argv = "cat-file --batch";
    let mut child = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(root)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env_remove("GIT_INDEX_FILE")
        .args(["cat-file", "--batch"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|source| SnapshotError::Spawn {
            argv: argv.to_string(),
            source,
        })?;

    let requests: String = wanted.iter().map(|(oid, _)| format!("{oid}\n")).collect();
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(requests.as_bytes());
            let _ = stdin.flush();
        });
    }

    let output = child
        .wait_with_output()
        .map_err(|source| SnapshotError::Spawn {
            argv: argv.to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(SnapshotError::Git {
            argv: argv.to_string(),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(parse_cat_file_batch(&output.stdout, wanted.len()))
}

/// Parse `cat-file --batch` output: `<oid> SP <type> SP <size> LF <contents> LF`.
///
/// Framed by the declared byte length, never by scanning for a delimiter — a
/// blob may contain anything, including the header shape of the next record.
fn parse_cat_file_batch(stdout: &[u8], expected: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(expected);
    let mut cursor = 0usize;
    while out.len() < expected && cursor < stdout.len() {
        let Some(nl) = stdout[cursor..].iter().position(|b| *b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&stdout[cursor..cursor + nl]).to_string();
        cursor += nl + 1;
        let mut fields = header.split_whitespace();
        let size = match (fields.next(), fields.next(), fields.next()) {
            // `<oid> missing` — one field after the oid, no size.
            (Some(_), Some("missing"), None) => {
                out.push(String::new());
                continue;
            }
            (Some(_), Some(_), Some(size)) => size.parse::<usize>().unwrap_or(0),
            _ => break,
        };
        let end = (cursor + size).min(stdout.len());
        out.push(String::from_utf8_lossy(&stdout[cursor..end]).to_string());
        // Skip the contents and the trailing newline git appends.
        cursor = (end + 1).min(stdout.len());
    }
    // Any record git did not answer is an empty (unreadable) attributes file.
    out.resize(expected, String::new());
    out
}

/// Pull `filter=<name>` and the other content-altering attributes out of one
/// attributes file.
fn scan_attributes(text: &str, source: &str, into: &mut ContentFilters) {
    let mut hit = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Skip the pattern; attributes are whitespace-separated after it.
        for token in line.split_whitespace().skip(1) {
            let token = token.trim_start_matches('-').trim_start_matches('!');
            if let Some(name) = token.strip_prefix("filter=") {
                if !name.is_empty() {
                    into.drivers.insert(name.to_string());
                    hit = true;
                }
            } else if token == "ident" {
                into.other_transforms.insert("ident".to_string());
                hit = true;
            } else if token == "text" || token.starts_with("text=") {
                into.other_transforms.insert("text".to_string());
                hit = true;
            } else if token.starts_with("eol=") {
                into.other_transforms.insert("eol".to_string());
                hit = true;
            }
        }
    }
    if hit {
        into.sources.push(source.to_string());
    }
}

/// The repository root containing `path`.
fn resolve_repo_root(path: &Path) -> Result<PathBuf, SnapshotError> {
    if !path.exists() {
        return Err(SnapshotError::NotARepository {
            path: path.to_path_buf(),
            reason: "path does not exist".to_string(),
        });
    }
    let out = run_git(path, &["rev-parse", "--show-toplevel"], None, &[]).map_err(|e| {
        SnapshotError::NotARepository {
            path: path.to_path_buf(),
            reason: e.to_string(),
        }
    })?;
    let root = out.trim();
    if root.is_empty() {
        return Err(SnapshotError::NotARepository {
            path: path.to_path_buf(),
            reason: "git printed no toplevel".to_string(),
        });
    }
    Ok(PathBuf::from(root))
}

/// `-c core.excludesFile=…` arguments, or nothing.
///
/// Returns an empty vector when there is no `.thrulineignore`, so the common
/// case does not go near the user's excludes configuration at all.
fn combined_excludes_file(root: &Path, scratch: &Path) -> Result<Vec<String>, SnapshotError> {
    let thruline_ignore = root.join(THRULINE_IGNORE);
    if !thruline_ignore.is_file() {
        return Ok(Vec::new());
    }

    let mut combined = String::new();
    // The user's own global excludes come first so that a `!negation` in
    // `.thrulineignore` can re-include something they globally ignore. Git
    // applies the last matching pattern in a file.
    if let Ok(configured) = run_git(root, &["config", "--get", "core.excludesFile"], None, &[]) {
        let configured = expand_tilde(configured.trim());
        if let Ok(text) = std::fs::read_to_string(&configured) {
            combined.push_str(&text);
            combined.push('\n');
        }
    }
    match std::fs::read_to_string(&thruline_ignore) {
        Ok(text) => {
            combined.push_str(&text);
            combined.push('\n');
        }
        Err(error) => {
            // Fail loud rather than quietly snapshotting files the user asked
            // to keep out: an unreadable .thrulineignore is a real condition.
            return Err(SnapshotError::TempDir(error));
        }
    }

    let path = scratch.join("thruline-excludes");
    std::fs::write(&path, combined).map_err(SnapshotError::TempDir)?;
    Ok(vec![
        "-c".to_string(),
        format!("core.excludesFile={}", path.display()),
    ])
}

/// `~` expansion, because `core.excludesFile` is conventionally `~/.gitignore`
/// and git stores it unexpanded.
fn expand_tilde(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(rest),
            None => PathBuf::from(path),
        },
        None => PathBuf::from(path),
    }
}

/// Run git in `cwd`, optionally against a private index.
fn run_git(
    cwd: &Path,
    args: &[&str],
    index: Option<&Path>,
    prefix_args: &[String],
) -> Result<String, SnapshotError> {
    let argv = || {
        prefix_args
            .iter()
            .map(String::as_str)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut command = Command::new("git");
    crate::git_isolation::isolate_config(&mut command);
    command.current_dir(cwd);
    // Force the C locale. `nested_repository_path` reads git's stderr, and a
    // translated message would silently stop matching — the failure would be
    // loud (the generic `Git` error still carries the stderr verbatim) but the
    // actionable message would be lost on every non-English machine.
    command.env("LC_ALL", "C");
    command.env("LANG", "C");
    for arg in prefix_args {
        command.arg(arg);
    }
    command.args(args);
    match index {
        Some(path) => {
            command.env("GIT_INDEX_FILE", path);
        }
        // Explicitly clear it: if the *caller's* environment already has
        // GIT_INDEX_FILE set (git sets it for hooks, and ThruLine runs inside
        // hooks), a read-only command would otherwise consult a foreign index.
        None => {
            command.env_remove("GIT_INDEX_FILE");
        }
    }
    let output = command.output().map_err(|source| SnapshotError::Spawn {
        argv: argv(),
        source,
    })?;
    if !output.status.success() {
        return Err(SnapshotError::Git {
            argv: argv(),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Accept only a full hex object id — SHA-1 (40) or SHA-256 (64).
fn object_id(output: &str, argv: &str) -> Result<String, SnapshotError> {
    let id = output.trim();
    let ok = matches!(id.len(), 40 | 64) && id.bytes().all(|b| b.is_ascii_hexdigit());
    if !ok {
        return Err(SnapshotError::NotAnObjectId {
            argv: argv.to_string(),
            output: id.to_string(),
        });
    }
    Ok(id.to_string())
}

fn count_lines(output: &str) -> u64 {
    output.lines().filter(|l| !l.trim().is_empty()).count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ids_must_be_full_hashes() {
        assert!(object_id(&"a".repeat(40), "x").is_ok());
        assert!(object_id(&"0".repeat(64), "x").is_ok());
        for bad in ["", "abc", &"z".repeat(40), &"a".repeat(39), &"a".repeat(41)] {
            assert!(
                object_id(bad, "x").is_err(),
                "{bad:?} accepted as an object id"
            );
        }
    }

    #[test]
    fn line_counting_ignores_blank_output() {
        assert_eq!(count_lines(""), 0);
        assert_eq!(count_lines("\n"), 0);
        assert_eq!(count_lines("a\nb\n"), 2);
    }

    #[test]
    fn tilde_expands_against_home() {
        // Not `is_absolute`: on a machine with no HOME the input is returned
        // unchanged, and asserting absoluteness would fail there for the wrong
        // reason.
        let expanded = expand_tilde("~/.gitignore");
        assert!(
            !expanded.to_string_lossy().starts_with("~/"),
            "{expanded:?}"
        );
        assert_eq!(expand_tilde("/etc/x"), PathBuf::from("/etc/x"));
    }
}
