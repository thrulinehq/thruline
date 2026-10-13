//! P2b — the embedded code digest is verified, not asserted.
//!
//! `crates/cli/build.rs` stamps the binary with a digest of the engine source it
//! was compiled from, and it computes that digest with its own copy of the
//! algorithm — a build script cannot depend on the crate it is building without
//! compiling the whole engine for the host. Two copies of a hash function is
//! exactly the kind of duplication that drifts silently and then produces two
//! confident, different answers about one tree.
//!
//! So the copies are pinned together here: walk the tree with the core
//! implementation, and require the result to equal what the build script put in
//! the binary. If either copy changes, this fails.

use std::path::PathBuf;

use thruline_core::perf_basis::{code_digest, engine_sources};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_embedded_digest_matches_a_fresh_walk_of_the_tree() {
    let embedded = env!("THRULINE_CODE_DIGEST");
    assert_ne!(
        embedded, "unavailable",
        "the build script could not read the source tree, so no figure can ever \
         be current. That is the honest fallback, but it is not the expected \
         state inside this repository."
    );
    let mut entries = engine_sources(&repo_root()).expect("walking the engine source");
    let fresh = code_digest(&mut entries);
    assert_eq!(
        embedded, fresh,
        "the digest embedded by crates/cli/build.rs disagrees with the digest \
         computed by thruline_core::perf_basis. The two copies of the algorithm \
         (or of the file set) have drifted."
    );
}

/// The file set is not empty and covers both crates — a digest over three
/// manifests would be stable, cheap, and would answer the wrong question.
#[test]
fn the_file_set_covers_the_engine_and_not_only_its_manifests() {
    let entries = engine_sources(&repo_root()).expect("walking the engine source");
    let paths: Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();
    assert!(paths.contains(&"crates/core/src/ledger.rs"), "{paths:?}");
    assert!(paths.contains(&"crates/core/src/export.rs"), "{paths:?}");
    assert!(paths.contains(&"crates/cli/src/main.rs"), "{paths:?}");
    assert!(paths.contains(&"Cargo.lock"), "{paths:?}");
    assert!(
        entries.len() > 50,
        "only {} files hashed — the walk is not reaching the engine",
        entries.len()
    );
}

/// A change to a documentation file must NOT move the digest. This is the
/// decision that makes the stamp usable: E2's wording is "when the code beneath
/// it changes", and a stamp that expired on every markdown commit would be
/// switched off within a day.
#[test]
fn documentation_is_outside_the_digest() {
    let entries = engine_sources(&repo_root()).expect("walking the engine source");
    assert!(
        !entries.iter().any(|(p, _)| p.starts_with("docs/")),
        "a docs/ file is inside the code digest"
    );
    assert!(
        !entries.iter().any(|(p, _)| p.starts_with("ui/")),
        "a ui/ file is inside the code digest; the perf figures measure the engine"
    );
}
