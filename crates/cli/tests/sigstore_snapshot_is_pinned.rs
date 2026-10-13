//! `crates/sigstore` is what `crates/sigstore/VENDORED.md` says it is.
//!
//! It is a pinned copy of Track B's `~/thruline-sigstore` — a **separate
//! repository**, whose commits are not objects here, which is why it is a
//! snapshot and not a merge. A pin nobody recomputes is a comment, so this
//! recomputes it on every gate.
//!
//! # It lives outside the crate it checks, deliberately
//!
//! Putting it in `crates/sigstore/tests/` would have added a core-authored file
//! to a tree whose whole claim is that it is byte-identical to B's — and it
//! would have been inside its own digest. The snapshot stays pure; the core
//! asserts about it from its own lane.
//!
//! # What this catches, and what it does not
//!
//! It catches the tree drifting from the digest recorded beside it: an edit made
//! in place, a file added or removed, a bad re-vendoring. It does **not** check
//! the digest against B's repository at the named commit — that repository is
//! not on every machine that runs this gate, and a test that silently skips
//! where it matters is the failure this codebase keeps meeting. Re-vendoring is
//! what establishes that link, and `VENDORED.md` records the command.

use std::collections::HashSet;
use std::path::PathBuf;
use thruline_core::chain::sha256_hex;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .join("crates/sigstore")
}

/// Every travelled file, `LC_ALL=C`-sorted by relative path, excluding the pin
/// file itself — which cannot hash itself — and any build output.
fn travelled_files() -> Vec<(String, PathBuf)> {
    let root = crate_root();
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                // `target` is build output. `__pycache__` is too, and it is
                // worse: `.gitignore` keeps it out of the commit, so pinning it
                // recorded a digest over files a fresh clone would never see.
                if p.file_name()
                    .is_some_and(|n| n == "target" || n == "__pycache__")
                {
                    continue;
                }
                stack.push(p);
                continue;
            }
            if p.file_name().is_some_and(|n| n == "VENDORED.md") {
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, p));
        }
    }
    out.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    out
}

/// The manifest-of-hashes construction `VENDORED.md` documents, so a reader can
/// reproduce it with `shasum` and no Rust at all.
fn tree_digest() -> (String, usize) {
    let files = travelled_files();
    let mut manifest = String::new();
    for (rel, path) in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        manifest.push_str(&format!("{}  {}\n", sha256_hex(&bytes), rel));
    }
    (sha256_hex(manifest.as_bytes()), files.len())
}

fn pin_field(key: &str) -> String {
    let text = std::fs::read_to_string(crate_root().join("VENDORED.md")).expect("VENDORED.md");
    text.lines()
        .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
        .unwrap_or_else(|| panic!("VENDORED.md states no `{key}`"))
}

#[test]
fn the_vendored_tree_matches_the_digest_recorded_beside_it() {
    let (actual, count) = tree_digest();
    let recorded = pin_field("travelled-tree      sha256:");
    assert_eq!(
        actual, recorded,
        "crates/sigstore has drifted from its own VENDORED.md.\n\n\
         This crate is a PINNED COPY of Track B's separate repository. An edit \
         made here is not B's code and is lost at the next re-vendoring. \
         Re-vendor from a named B commit and update the pin in the same change."
    );

    let recorded_count: usize = pin_field("files travelled     ")
        .parse()
        .expect("files travelled is a number");
    assert_eq!(
        count, recorded_count,
        "the pin says {recorded_count} travelled files and {count} are here"
    );
}

/// The one exclusion is deliberate and stays deliberate.
#[test]
fn the_core_contract_test_did_not_travel() {
    assert!(
        !crate_root().join("tests/core_contract.rs").exists(),
        "tests/core_contract.rs is B's assertion about the CORE, run against a \
         real core checkout. Inside the core it would assert things about the \
         tree it lives in, which is a much weaker statement. It stays with B."
    );
}

/// A pin with no source commit pins nothing.
#[test]
fn the_pin_names_its_source() {
    let commit = pin_field("source commit       ");
    assert!(
        commit.len() >= 7 && commit.chars().all(|c| c.is_ascii_hexdigit()),
        "VENDORED.md's source commit is {commit:?}, which is not a commit id"
    );
}

/// A pinned file that git never tracked is pinned only on the machine that
/// vendored it.
///
/// This is the defect that made the first recording of this pin wrong.
/// `.gitignore:13` is `__pycache__/`, so `git add` skipped two of B's fixture
/// files without a word, and the digest went into `VENDORED.md` describing a
/// 121-file worktree while the commit carried 119. It was green here and would
/// have been red on every clone — and red on the *count*, which reads like a
/// bad vendoring rather than like the truth.
///
/// Excluding `__pycache__` from the walk fixes that one case. This fixes the
/// class: the next ignored pattern B happens to commit — a log, a `.DS_Store`,
/// a build directory this repo ignores and theirs does not — is caught the
/// moment it is vendored, on the vendoring machine, instead of on someone
/// else's clone.
#[test]
fn every_pinned_file_is_tracked_by_git() {
    let root = crate_root();
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", "."])
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("could not run git in {}: {e}", root.display()));
    assert!(
        out.status.success(),
        "`git ls-files` failed in {}: {}\n\nThis test does not skip when it \
         cannot look. A pin nobody can check against the commit is the failure \
         it exists to catch.",
        root.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );

    let tracked: HashSet<String> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).replace('\\', "/"))
        .collect();

    let untracked: Vec<String> = travelled_files()
        .into_iter()
        .map(|(rel, _)| rel)
        .filter(|rel| !tracked.contains(rel))
        .collect();

    assert!(
        untracked.is_empty(),
        "{} file(s) are inside the digest but not in the commit: {untracked:?}\n\n\
         The pin would be green here and red on a fresh clone, because those \
         bytes exist only in this worktree. Either the file belongs in the \
         snapshot — in which case something in .gitignore is refusing it and \
         needs an exception — or it does not, in which case the walk should \
         skip it the way it skips `target` and `__pycache__`.",
        untracked.len()
    );
}
