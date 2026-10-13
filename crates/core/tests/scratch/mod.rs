//! A temp tree that can still be removed after A208 has sealed it.
//!
//! `tempfile::TempDir`'s destructor calls `remove_dir_all` and **ignores the
//! error**. A208's protected store seals `mirror/generations/**` — `seal.part`
//! and `payload.part` at `0o400`, under directories it also seals — so the
//! removal fails with `EACCES` and the destructor says nothing at all. The
//! directory then survives for the life of the machine.
//!
//! That is not a hypothesis. One day's gate `TMPDIR` was measured at 3.7 GiB
//! across 5,775 entries, of which 4,793 were hidden `.tmp*` roots holding
//! 2,871 MB; `rm -rf` refused 5,020 of them with `Permission denied` on those
//! two filenames, and clearing the read-only bit first removed every one
//! (DECISIONS #137).
//!
//! **A208's locking is the product's contract and is not touched here.** The
//! store is supposed to be read-only; what was wrong is the disposal. This
//! clears the bit on the way out, and only on a tree a test owns.

#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// A `TempDir` whose removal is not defeated by a sealed store inside it.
///
/// `Drop` on this struct runs **before** its fields are dropped, which is the
/// whole mechanism: the tree is unlocked, and only then does the inner
/// `TempDir` try to remove it.
pub struct TempTree {
    dir: tempfile::TempDir,
}

impl TempTree {
    pub fn new() -> TempTree {
        TempTree {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn join(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }
}

impl Default for TempTree {
    fn default() -> Self {
        TempTree::new()
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        unlock(self.dir.path());
    }
}

/// Clear the read-only bit over a whole tree, descending as it goes.
///
/// A directory must be writable **and** executable before its children can be
/// reached or unlinked, so each directory is unlocked on the way down rather
/// than on the way back up. Every error is deliberately ignored: this runs in
/// a destructor, and a scratch tree that cannot be unlocked is reported by the
/// guard that checks the *consequence*, not by a panic during unwinding.
fn unlock(dir: &Path) {
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            unlock(&path);
        } else if !kind.is_symlink() {
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
}
