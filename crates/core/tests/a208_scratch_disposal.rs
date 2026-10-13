//! DECISIONS #137 — a run that seals the protected store must leave nothing.
//!
//! `tempfile::TempDir`'s destructor calls `remove_dir_all` and ignores its
//! error. A208 seals `mirror/generations/**` at `0o400` under directories it
//! also seals, so that removal fails with `EACCES` **silently** and the tree
//! survives. Measured once as 4,793 orphaned roots holding 2,871 MB in a
//! single day's gate `TMPDIR`.
//!
//! # This asserts the consequence, not the operation
//!
//! The tempting guard — "the cleanup returned `Ok`" — is exactly the guard this
//! failure walks past, because the cleanup is *entitled* to swallow its error
//! and `TempDir`'s does. Track C reproduced the same cause against its own
//! reaper: `shutil.rmtree(root, ignore_errors=True)` over a `0o400` file under
//! a `0o500` directory raises nothing and leaves the root standing. So the
//! cleanup may swallow; the check may not. **This asserts the root is gone.**

mod scratch;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Seal a transcript into a mirror under `root`, the way C1.3's pre-compaction
/// seal does, and return the sealed store's directory.
fn seal_a_generation(root: &Path) -> PathBuf {
    let project_dir = root.join("-tmp-a208-project");
    std::fs::create_dir_all(&project_dir).expect("mkdir");
    let transcript = project_dir.join("11111111-2222-4333-8444-555555555555.jsonl");
    std::fs::write(&transcript, b"{\"type\":\"user\"}\n").expect("write transcript");
    let mirror_root = root.join("mirror");
    let db = root.join("ledger.db");
    let mut ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let out = thruline_core::ingest::seal_transcript(&mut ledger, &mirror_root, &transcript)
        .expect("seal");
    assert_eq!(
        out.claude_sealed, 1,
        "nothing was sealed, so this test would pass without exercising the \
         read-only path it exists for"
    );
    mirror_root
}

/// Every sealed file under `dir`, with its mode.
fn sealed_modes(dir: &Path) -> Vec<(PathBuf, u32)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(m) = std::fs::metadata(&p) {
                out.push((p, m.permissions().mode() & 0o777));
            }
        }
    }
    out
}

#[test]
fn a_scratch_tree_that_sealed_the_protected_store_leaves_nothing_behind() {
    let path = {
        let s = scratch::TempTree::new();
        let mirror_root = seal_a_generation(s.path());

        // The control. If the store were not actually read-only, removal would
        // be ordinary and this test would prove nothing about the failure it
        // guards. A208's contract is asserted here, not changed.
        let modes = sealed_modes(&mirror_root);
        assert!(
            !modes.is_empty(),
            "the seal produced no files under {}",
            mirror_root.display()
        );
        assert!(
            modes.iter().any(|(_, m)| *m == 0o400),
            "no file under the sealed store is 0o400, so A208's locking is not \
             in force here and this test is not exercising the EACCES path that \
             orphaned 4,793 temp roots:\n{modes:#?}"
        );

        s.path().to_path_buf()
    };

    assert!(
        !path.exists(),
        "the scratch root survived its own destructor: {}\n\n\
         SCOPE: this is the A208 protected-store leak (DECISIONS #137). A208 \
         seals `mirror/generations/**` at 0o400 under directories it also \
         seals; `TempDir`'s destructor calls `remove_dir_all`, hits EACCES, and \
         IGNORES the error, so the tree survives for the life of the machine. \
         Measured once at 4,793 orphaned roots / 2,871 MB in one day of gates. \
         The fix is disposal-side only — `tests/scratch/mod.rs` clears the \
         read-only bit before the TempDir drops. A208's locking is the \
         product's contract and must NOT be relaxed to make this pass.",
        path.display()
    );
}
