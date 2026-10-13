//! Materialise a git tree into a directory — **without `git checkout`**.
//!
//! # The standing ruling this file implements
//!
//! BUILD-PLAN C.7/4: *"Re-execution materialises without filters. Never a plain
//! `git checkout` — write blobs directly via `git cat-file` into the target, so
//! the container holds exactly the tree bytes and smudge cannot inject."*
//!
//! The reason is not style. `git checkout` runs the **smudge** half of any
//! `.gitattributes` filter pair, plus `text`/`eol` normalisation and `ident`
//! substitution. Its output is therefore *not a function of the tree hash*: the
//! same tree checked out under two different configurations yields two
//! different working trees, and a smudge filter can inject content that exists
//! in no tree at all. Re-executing tests against that and stamping
//! `reexecuted: true` would be the laundering path C.7 was written to close.
//!
//! `git cat-file` reads the object. There is no filter in that path, and the
//! bytes written here are the bytes the tree names — which is the only property
//! §7 can build on.
//!
//! A source-walking test (`crates/reexec/tests/git_verbs.rs`) enforces this
//! mechanically, in the same shape as the §4.4 snapshot engine's narrowed
//! forbidden-verb guard: `checkout`, `checkout-index`, `read-tree`, `restore`,
//! `archive` and `worktree` must not appear here, and `cat-file` must.
//!
//! # What cannot be materialised, and why it refuses rather than warns
//!
//! A **gitlink** (mode `160000`, a submodule) names a commit in another
//! repository. The parent tree carries the commit id and none of its content
//! (C.7 row 8), so a directory built from the parent tree is *missing source
//! the tests may import*. Materialisation records every gitlink; the caller
//! (`Sandbox::prepare`) refuses on a non-empty list. A re-execution over a tree
//! with a hole in it that passed would be evidence about code that was not
//! there.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What a materialisation produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Materialized {
    /// Regular files written, by repo-relative path, with their blob oid.
    pub files: BTreeMap<String, String>,
    /// Symlinks written, path → target.
    pub symlinks: BTreeMap<String, String>,
    /// Submodule entries (mode `160000`), path → commit id. **Not written**:
    /// their content is not in this tree.
    pub gitlinks: BTreeMap<String, String>,
    /// Total bytes written.
    pub bytes: u64,
}

impl Materialized {
    /// Is every path this tree names actually present in the target?
    ///
    /// False when a gitlink was seen. Deliberately not a warning — see the
    /// module docs.
    pub fn complete(&self) -> bool {
        self.gitlinks.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MaterializeError {
    #[error("`git {argv}` failed with {status}: {stderr}")]
    Git {
        argv: String,
        status: String,
        stderr: String,
    },
    #[error("running git: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("writing {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("`git ls-tree` produced a line this build cannot read: {line:?}")]
    Unreadable { line: String },
    #[error(
        "the tree names {path:?}, which escapes the target directory. Refused: a tree entry \
         that can write outside the sandbox is not a tree entry this build will honour."
    )]
    Escapes { path: String },
    #[error(
        "`git cat-file` returned {found} bytes for {oid} where the object header said {expected}"
    )]
    ShortObject {
        oid: String,
        expected: u64,
        found: u64,
    },
    #[error("`git cat-file` says {oid} is {kind:?}, not a blob")]
    NotABlob { oid: String, kind: String },
    #[error(
        "the tree names {path:?}, and the component {component:?} of it is a symlink this \
         materialisation wrote a moment ago. Refused: the write would follow that link and \
         land wherever it points — outside this directory. A tree entry whose bytes end up \
         outside the tree's own directory is not a function of the tree hash, which is what \
         §0.3 binds every check to."
    )]
    SymlinkedParent { path: String, component: String },
    #[error(
        "the tree names {path:?}, and this filesystem already holds that path — two tree \
         entries that differ to git ({other:?}) collide here, as a case-insensitive or \
         Unicode-normalising filesystem makes them. Refused: the second write would land \
         through whatever the first entry is, and if the first was a symlink that is outside \
         this directory."
    )]
    Collision { path: String, other: String },
}

/// One entry of `git ls-tree -r -z`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    mode: String,
    oid: String,
    path: String,
}

/// Write every blob of `tree_hash` into `target`.
///
/// `target` must exist and should be empty; this function creates
/// subdirectories as it goes and never deletes anything.
pub fn materialize(
    repo: &Path,
    tree_hash: &str,
    target: &Path,
) -> Result<Materialized, MaterializeError> {
    let entries = list_tree(repo, tree_hash)?;
    let mut out = Materialized::default();

    // One `cat-file --batch` process for the whole tree. Spawning one per blob
    // is the shape that made `verify`'s object lookup cost 1.9 s; the batch
    // protocol exists precisely so a caller does not have to.
    let mut child = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["cat-file", "--batch"])
        .env_remove("GIT_INDEX_FILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(MaterializeError::Spawn)?;
    let mut stdin = match child.stdin.take() {
        Some(s) => s,
        None => {
            return Err(MaterializeError::Spawn(std::io::Error::other(
                "git cat-file gave no stdin",
            )))
        }
    };
    let mut stdout = BufReader::new(match child.stdout.take() {
        Some(s) => s,
        None => {
            return Err(MaterializeError::Spawn(std::io::Error::other(
                "git cat-file gave no stdout",
            )))
        }
    });

    for entry in &entries {
        // A gitlink names a commit in another repository; its content is not
        // here. Recorded and skipped — the caller refuses on a non-empty list.
        if entry.mode == "160000" {
            out.gitlinks.insert(entry.path.clone(), entry.oid.clone());
            continue;
        }
        let path = safe_join(target, &entry.path)?;
        // Not `create_dir_all`: it is satisfied by a symlink-to-a-directory,
        // which is how a byte written "into" this tree lands somewhere else
        // entirely. Every ancestor is created here, or proven to be a real
        // directory, one component at a time.
        create_dirs_no_follow(target, &entry.path)?;

        stdin
            .write_all(format!("{}\n", entry.oid).as_bytes())
            .map_err(MaterializeError::Spawn)?;
        stdin.flush().map_err(MaterializeError::Spawn)?;
        let (kind, size) = read_header(&mut stdout, &entry.oid)?;
        if kind != "blob" {
            return Err(MaterializeError::NotABlob {
                oid: entry.oid.clone(),
                kind,
            });
        }
        let mut body = vec![0u8; size as usize];
        stdout
            .read_exact(&mut body)
            .map_err(|_| MaterializeError::ShortObject {
                oid: entry.oid.clone(),
                expected: size,
                found: 0,
            })?;
        // The batch protocol writes a trailing newline after each object.
        let mut nl = [0u8; 1];
        let _ = stdout.read_exact(&mut nl);

        if entry.mode == "120000" {
            // The blob content IS the link target.
            let target_text = String::from_utf8_lossy(&body).to_string();
            write_symlink(&path, &target_text).map_err(|e| collided(e, &entry.path, &out))?;
            out.symlinks.insert(entry.path.clone(), target_text);
            out.bytes += size;
            continue;
        }

        // `create_new`: O_CREAT|O_EXCL never follows a symlink at the final
        // component and never overwrites. `fs::write` does both — it is how the
        // colliding entry below `Evil` used to be written through the link.
        write_new(&path, &body, entry.mode == "100755")
            .map_err(|e| collided(e, &entry.path, &out))?;
        out.files.insert(entry.path.clone(), entry.oid.clone());
        out.bytes += size;
    }

    drop(stdin);
    let _ = child.wait();
    Ok(out)
}

/// Read one `<oid> SP <type> SP <size> LF` header from the batch stream.
fn read_header<R: BufRead>(stdout: &mut R, oid: &str) -> Result<(String, u64), MaterializeError> {
    let mut line = String::new();
    stdout
        .read_line(&mut line)
        .map_err(MaterializeError::Spawn)?;
    let mut fields = line.split_whitespace();
    let (_oid, kind, size) = (fields.next(), fields.next(), fields.next());
    match (kind, size.and_then(|s| s.parse::<u64>().ok())) {
        (Some(kind), Some(size)) => Ok((kind.to_string(), size)),
        _ => Err(MaterializeError::NotABlob {
            oid: oid.to_string(),
            kind: line.trim().to_string(),
        }),
    }
}

/// `git ls-tree -r -z <tree>`, parsed.
fn list_tree(repo: &Path, tree_hash: &str) -> Result<Vec<Entry>, MaterializeError> {
    let out = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["ls-tree", "-r", "-z", tree_hash])
        .env_remove("GIT_INDEX_FILE")
        .output()
        .map_err(MaterializeError::Spawn)?;
    if !out.status.success() {
        return Err(MaterializeError::Git {
            argv: format!("ls-tree -r -z {tree_hash}"),
            status: out.status.to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut entries = Vec::new();
    for record in text.split('\0') {
        if record.is_empty() {
            continue;
        }
        // `<mode> SP <type> SP <oid> TAB <path>`
        let (meta, path) = match record.split_once('\t') {
            Some(pair) => pair,
            None => {
                return Err(MaterializeError::Unreadable {
                    line: record.to_string(),
                })
            }
        };
        let mut fields = meta.split_whitespace();
        let (mode, _type, oid) = (fields.next(), fields.next(), fields.next());
        match (mode, oid) {
            (Some(mode), Some(oid)) => entries.push(Entry {
                mode: mode.to_string(),
                oid: oid.to_string(),
                path: path.to_string(),
            }),
            _ => {
                return Err(MaterializeError::Unreadable {
                    line: record.to_string(),
                })
            }
        }
    }
    Ok(entries)
}

/// Join a tree path onto the target, refusing anything that escapes it.
///
/// A git tree cannot legitimately contain `..` or an absolute path — git
/// refuses to write one. This is not defence against git; it is defence against
/// **whatever produced the tree**, which in this system is by assumption an
/// adversary. A path that escapes writes outside the sandbox.
fn safe_join(target: &Path, rel: &str) -> Result<PathBuf, MaterializeError> {
    let escapes = |p: &str| {
        p.is_empty()
            || p.starts_with('/')
            || p.split('/')
                .any(|c| c == ".." || c == "." || c.is_empty())
            // A Windows-style absolute or drive-relative path.
            || p.contains('\\')
            || p.chars().nth(1) == Some(':')
    };
    if escapes(rel) {
        return Err(MaterializeError::Escapes {
            path: rel.to_string(),
        });
    }
    Ok(target.join(rel))
}

/// Create every ancestor directory of `rel` under `target`, refusing to walk
/// through anything that is not a real directory.
///
/// The escape this closes: a git tree cannot name both `evil` and `evil/x` —
/// the names collide. It *can* name `Evil` (a symlink) and `evil/x`, which are
/// two entries to git and **one path** on a case-insensitive or
/// Unicode-normalising filesystem (macOS APFS by default; Windows). `ls-tree
/// -r` emits `Evil` first, so by the time `evil/x` is written the symlink is
/// already planted and `create_dir_all` is satisfied by it.
///
/// `symlink_metadata` does not follow, so a symlink standing where a directory
/// should be is seen for what it is and refused. Refused, not skipped: git
/// checkout-index drops the entry, but a materialisation missing a file the
/// tree names would run tests against a tree with a hole in it, and this crate
/// already treats that (the gitlink case) as a refusal rather than a warning.
fn create_dirs_no_follow(target: &Path, rel: &str) -> Result<(), MaterializeError> {
    let mut cur = target.to_path_buf();
    let mut components: Vec<&str> = rel.split('/').collect();
    components.pop(); // the entry itself, not a directory to create
    for component in components {
        cur.push(component);
        match std::fs::symlink_metadata(&cur) {
            Ok(md) if md.file_type().is_dir() => {}
            Ok(_) => {
                return Err(MaterializeError::SymlinkedParent {
                    path: rel.to_string(),
                    component: component.to_string(),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&cur).map_err(|source| MaterializeError::Write {
                    path: cur.clone(),
                    source,
                })?;
            }
            Err(source) => {
                return Err(MaterializeError::Write {
                    path: cur.clone(),
                    source,
                })
            }
        }
    }
    Ok(())
}

/// Turn an `AlreadyExists` into the collision it is, naming the earlier entry.
///
/// Anything else passes through unchanged: a disk-full is not a collision, and
/// saying so would be the fail-quiet this crate exists to avoid.
fn collided(e: MaterializeError, rel: &str, out: &Materialized) -> MaterializeError {
    let already_exists = matches!(
        &e,
        MaterializeError::Write { source, .. } if source.kind() == std::io::ErrorKind::AlreadyExists
    );
    if !already_exists {
        return e;
    }
    let other = out
        .files
        .keys()
        .chain(out.symlinks.keys())
        .find(|p| p.eq_ignore_ascii_case(rel))
        .cloned()
        .unwrap_or_else(|| "an entry written earlier in this tree".to_string());
    MaterializeError::Collision {
        path: rel.to_string(),
        other,
    }
}

/// Write a blob to a path that must not already exist, never following a link.
fn write_new(path: &Path, body: &[u8], executable: bool) -> Result<(), MaterializeError> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| MaterializeError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(body)
        .map_err(|source| MaterializeError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    if executable {
        set_executable(&file, path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn write_symlink(path: &Path, target: &str) -> Result<(), MaterializeError> {
    std::os::unix::fs::symlink(target, path).map_err(|source| MaterializeError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn write_symlink(path: &Path, target: &str) -> Result<(), MaterializeError> {
    // Windows is tier 2 (§2): the build must pass, capture may be reduced. A
    // symlink becomes a regular file holding the target text, which is what git
    // itself does without `core.symlinks`.
    write_new(path, target.as_bytes(), false)
}

/// Set the mode on the **open handle**, not on the path.
///
/// `fs::set_permissions` resolves the path again and follows symlinks; the
/// handle cannot be redirected after `create_new` returned it.
#[cfg(unix)]
fn set_executable(file: &std::fs::File, path: &Path) -> Result<(), MaterializeError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o755))
        .map_err(|source| MaterializeError::Write {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(not(unix))]
fn set_executable(_file: &std::fs::File, _path: &Path) -> Result<(), MaterializeError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_escaping_the_target_is_refused() {
        let root = Path::new("/tmp/x");
        for bad in [
            "../etc/passwd",
            "/etc/passwd",
            "a/../../b",
            "",
            "a//b",
            "./a",
        ] {
            assert!(
                safe_join(root, bad).is_err(),
                "{bad:?} was accepted as a tree path"
            );
        }
        assert!(safe_join(root, "a/b/c.py").is_ok());
    }

    #[test]
    fn a_tree_with_a_gitlink_is_not_complete() {
        let mut m = Materialized::default();
        assert!(m.complete());
        m.gitlinks.insert("vendor/dep".into(), "abc".into());
        assert!(
            !m.complete(),
            "a submodule's content is not in this tree, so the directory has a hole in it"
        );
    }
}
