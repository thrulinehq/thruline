//! The live-store guard (C29). Included by every `#[ignore]` probe that opens
//! a store at a path it takes from the environment, with
//! `#[path = "support/live_store.rs"] mod live_store;`.
//!
//! No probe may open anything under the founder's live store, not even
//! read-only: SQLite can create `-wal` and `-shm` beside the file it opens,
//! and an empty `-wal` appearing there is already a write to that store. A
//! probe measures an APFS clone (`cp -c`) instead, or goes through
//! `thruline ui`, which is the product opening its own store.
//!
//! The live store is `$HOME/.thruline`, resolved through symlinks, and also
//! `$THRULINE_HOME` when that is set to anything other than a scratch folder:
//! every `thruline` command treats a set `THRULINE_HOME` as its home
//! (`private_store::thruline_home`, `Ledger::default_path`), so an inherited
//! one names the store this machine actually writes to. A `THRULINE_HOME`
//! under the OS temp folder or `/tmp` is a scratch store a test or a
//! measurement made, and is not treated as live.

use std::path::{Path, PathBuf};

/// `path` resolved through symlinks: canonical when it exists, otherwise its
/// nearest existing ancestor made canonical with the rest joined back on, so
/// a store that does not exist yet is judged by where it would be created.
pub fn resolved(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|d| d.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let mut tail = Vec::new();
    let mut at = abs.as_path();
    loop {
        if let Ok(real) = at.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (at.parent(), at.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                at = parent;
            }
            _ => return abs,
        }
    }
}

/// The folders a scratch store may live in: the OS temp folder and `/tmp`.
pub fn scratch_roots() -> Vec<PathBuf> {
    vec![resolved(&std::env::temp_dir()), resolved(Path::new("/tmp"))]
}

/// Why opening `path` would touch the live store, or `None` when it would not.
pub fn live_store_refusal(
    path: &Path,
    home: Option<&Path>,
    thruline_home: Option<&Path>,
    scratch: &[PathBuf],
) -> Option<String> {
    let target = resolved(path);
    let mut roots = Vec::new();
    if let Some(home) = home {
        roots.push(resolved(&home.join(".thruline")));
    }
    if let Some(th) = thruline_home {
        let th = resolved(th);
        if !scratch.iter().any(|s| th.starts_with(s)) {
            roots.push(th);
        }
    }
    roots
        .into_iter()
        .find(|root| target.starts_with(root))
        .map(|root| {
            format!(
                "refused: {} is inside the live store {}. A probe never opens the live store, \
             not even read-only (SQLite can create -wal and -shm there). Clone it first \
             (cp -c {}/ledger.db <scratch>/ledger.db) and point the probe at the clone, \
             or measure through `thruline ui`.",
                path.display(),
                root.display(),
                root.display()
            )
        })
}

/// Panic, before anything is opened, when `path` lies in the live store.
pub fn refuse_live_store(path: &Path) {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let thruline_home = std::env::var_os("THRULINE_HOME").map(PathBuf::from);
    if let Some(why) = live_store_refusal(
        path,
        home.as_deref(),
        thruline_home.as_deref(),
        &scratch_roots(),
    ) {
        panic!("{why}");
    }
}
