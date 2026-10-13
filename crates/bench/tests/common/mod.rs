//! A temp root that removes itself, and the reason it has to be an object.
//!
//! # The leak this replaces
//!
//! Three sites in this crate's tests built a directory under the system temp
//! dir by name — `std::env::temp_dir().join(format!("tl-…"))` — and nothing ever
//! removed it. Measured on the development machine when Track C found it
//! (DECISIONS #158): **1,997 directories left behind**, 1,583 `tl-c2-*`, 234
//! `tl-holdout-*` and 180 `tl-acct-*`, growing with every gate run.
//!
//! One run of this suite leaks about 2,000 directories, and every contract gate
//! in a day is another 2,000. Nothing in the suite failed because of it, which
//! is exactly why it survived: a leak costs nothing to the run that causes it.
//!
//! # Why a guard object and not a `remove_dir_all` at the end
//!
//! A removal on the happy path is removed by the first `panic!`, and a test
//! binary's failing runs are the ones that leak most — an assertion that fires
//! skips every line after it. `Drop` runs while unwinding, so the root goes
//! whether the test passes, fails, or is a `?` that returns early.
//!
//! **`tempfile` is not used**, deliberately: this crate's dependency list is
//! short on purpose and the type below is eleven lines. Adding a dependency to
//! delete a directory is the trade going the wrong way.

use std::path::{Path, PathBuf};

/// A directory that exists for as long as this value does.
pub struct TempRoot(PathBuf);

// EVERY TEST BINARY COMPILES THIS MODULE SEPARATELY and uses a different subset
// of it, so a method used by `temp_roots.rs` is dead code in `class_two.rs`.
// Under `-D warnings` that fails the build for a method that is very much
// alive — the allow is about Rust's per-binary view of a shared test module,
// not about an unused API, and removing a method here because one binary does
// not call it would break another.
#[allow(dead_code)]
impl TempRoot {
    /// A fresh directory, unique per process and per call.
    pub fn new(tag: &str) -> TempRoot {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tl-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        // Still removed first: a previous process with the same pid may have
        // died before its `Drop` ran, and inheriting its files would make a
        // test's fixture depend on how the last run ended.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        TempRoot(dir)
    }

    /// The root itself.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// A path inside the root. The root owns it, so nothing here needs its own
    /// cleanup.
    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        // Best effort by necessity — `Drop` cannot report — but the guard test
        // in `tests/temp_roots.rs` asserts the directory is GONE rather than
        // that this call returned, which is the only form of the claim worth
        // making.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
