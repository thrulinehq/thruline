//! Reads of a materialised tree that refuse to leave it.
//!
//! # The escape this closes
//!
//! `materialize` was repaired so that *writing* a tree cannot land outside the
//! target directory. The **read** side was not. `std::fs::read`,
//! `Path::is_file` and `std::fs::copy` all follow symlinks, and every consumer
//! of the materialised tree used one of them — so a tree that legally names a
//! symlink had the bytes behind that link decide the dependency-layer key, the
//! image reference, the install command executed at build time, and the
//! contents of the build context. Measured on 2026-09-04: one git tree hash
//! producing two different dependency layers, and a host file's bytes inside
//! the built image.
//!
//! `materialize` *records* such an entry as a symlink and is right to; the
//! readers *resolved* it against the host filesystem, which is where the escape
//! lives. `SandboxError::TreeDivergence` cannot see it: it re-derives the layer
//! from the tree and compares it with the worktree's, and both sides follow the
//! same link to the same host file and agree.
//!
//! §0.3 binds every check to a tree hash. An environment decided by a file no
//! tree names is not bound to anything.
//!
//! # Refuse, do not resolve within the tree
//!
//! The alternative repair was to resolve links *inside* the materialised tree,
//! chroot-style. That is a second path-resolution implementation in the trusted
//! path of a security tool, and it has to get `..`, absolute targets, link
//! chains, symlink loops and case-folding right or it reintroduces the escape
//! it was written to close. Refusal has one failure mode — a legitimate
//! re-execution that will not run — and that is the direction a verdict is
//! allowed to move. A repository that genuinely symlinks its lockfile gets a
//! named refusal telling it so, not a silent wrong answer.
//!
//! # The boundary this does not claim to hold
//!
//! Resolution is by `symlink_metadata` per component, so there is a window
//! between the check and the open. That window is not defended, and is not
//! claimed to be: the directory being read was created by this process under
//! `tempfile` and is not shared. The adversary in this crate's model is **the
//! tree**, which plants its links before materialisation and cannot act during
//! it. A racing local process is a different threat and would need `O_NOFOLLOW`
//! and an `openat` walk, which is a dependency and an `unsafe` surface this
//! workspace forbids in first-party code.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum TreeReadError {
    #[error(
        "the tree names {rel:?}, and the component {component:?} of it is a symlink. Refused: \
         reading through it would take the bytes from wherever it points — a file no tree \
         names — and the environment a check runs in has to be a function of the tree hash \
         the receipt binds it to (§0.3)."
    )]
    Symlink { rel: String, component: String },
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Walk `rel` under `root`, refusing to traverse a symlink at any component.
///
/// `Ok(None)` means the path is simply not there — or is there as something
/// that is not a regular file (a directory where a file was expected). Absence
/// is not an error: most callers are asking "does this repository have a
/// lockfile", and the answer "no" is ordinary.
///
/// A **symlink** is never absence. It is a positive statement by the tree that
/// resolves outside it, and it refuses.
fn locate(root: &Path, rel: &str) -> Result<Option<PathBuf>, TreeReadError> {
    let mut cur = root.to_path_buf();
    let mut components = rel.split('/').peekable();
    while let Some(component) = components.next() {
        cur.push(component);
        let md = match std::fs::symlink_metadata(&cur) {
            Ok(md) => md,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(TreeReadError::Io { path: cur, source }),
        };
        if md.file_type().is_symlink() {
            return Err(TreeReadError::Symlink {
                rel: rel.to_string(),
                component: component.to_string(),
            });
        }
        let last = components.peek().is_none();
        if last {
            return Ok(md.is_file().then_some(cur));
        }
        if !md.is_dir() {
            return Ok(None);
        }
    }
    Ok(None)
}

/// The bytes of `rel` under `root`, or `None` if it is not there.
pub fn read(root: &Path, rel: &str) -> Result<Option<Vec<u8>>, TreeReadError> {
    let Some(path) = locate(root, rel)? else {
        return Ok(None);
    };
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(source) => Err(TreeReadError::Io { path, source }),
    }
}

/// The text of `rel` under `root`, or `None` if it is not there.
pub fn read_to_string(root: &Path, rel: &str) -> Result<Option<String>, TreeReadError> {
    let Some(bytes) = read(root, rel)? else {
        return Ok(None);
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| TreeReadError::Io {
            path: root.join(rel),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "not valid UTF-8 — a configuration file this build will not guess at",
            ),
        })
}

/// Is `rel` a regular file reachable without traversing a symlink?
///
/// **A symlink answers `false` here**, where [`read`] refuses. The difference
/// is deliberate and the callers are different: this one answers "which
/// ecosystem is this", a guess that decides a default test command and no
/// verdict, and a wrong guess there produces nothing. Every path that decides
/// what is *built* or *executed* goes through [`read`] and gets the refusal.
pub fn is_regular_file(root: &Path, rel: &str) -> bool {
    matches!(locate(root, rel), Ok(Some(_)))
}

/// Refuse if `rel` exists as a symlink; say nothing otherwise.
///
/// The loud half of [`is_regular_file`]: a caller that would otherwise treat a
/// symlinked lockfile as *absent* uses this first, so the tree's entry is
/// refused by name rather than quietly dropped (Rule 12).
pub fn refuse_symlink(root: &Path, rel: &str) -> Result<(), TreeReadError> {
    locate(root, rel).map(|_| ())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn a_symlinked_component_refuses_wherever_it_sits() {
        let host = tempfile::tempdir().expect("host");
        std::fs::write(host.path().join("secret"), "outside\n").expect("write");
        std::fs::create_dir(host.path().join("d")).expect("mkdir");
        std::fs::write(host.path().join("d/secret"), "outside\n").expect("write");

        let tree = tempfile::tempdir().expect("tree");
        symlink(host.path().join("secret"), tree.path().join("leaf")).expect("symlink");
        symlink(host.path().join("d"), tree.path().join("dir")).expect("symlink");

        // The final component.
        let e = read(tree.path(), "leaf").expect_err("must refuse");
        assert!(matches!(e, TreeReadError::Symlink { .. }), "{e}");
        assert!(!is_regular_file(tree.path(), "leaf"));

        // And an intermediate one, which the write-side guard never sees:
        // nothing was written beneath it.
        let e = read(tree.path(), "dir/secret").expect_err("must refuse");
        assert!(matches!(e, TreeReadError::Symlink { .. }), "{e}");
        assert!(!is_regular_file(tree.path(), "dir/secret"));
    }

    #[test]
    fn an_ordinary_file_still_reads_and_an_absent_one_is_not_an_error() {
        let tree = tempfile::tempdir().expect("tree");
        std::fs::create_dir(tree.path().join("sub")).expect("mkdir");
        std::fs::write(tree.path().join("sub/x"), "in the tree\n").expect("write");
        assert_eq!(
            read_to_string(tree.path(), "sub/x")
                .expect("read")
                .as_deref(),
            Some("in the tree\n")
        );
        assert!(is_regular_file(tree.path(), "sub/x"));
        assert_eq!(read(tree.path(), "sub/missing").expect("absent"), None);
        assert_eq!(read(tree.path(), "missing/x").expect("absent"), None);
        assert!(!is_regular_file(tree.path(), "sub"));
    }
}
