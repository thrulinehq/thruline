//! The narrowed forbidden-git-verb guard for the §4.4 snapshot engine.
//!
//! This repository has a standing doctrine that its git calls are read-only,
//! enforced by source-walking tests over two named modules
//! (`a206_workspace_identity.rs`, `a208_protected_store.rs`). BUILD-SPEC §4.4
//! requires two of the verbs on that list — `add` and `write-tree` — because a
//! tree hash covering untracked files cannot be produced without them.
//!
//! BUILD-PLAN C.6 is explicit that this is a **narrowing of a documented
//! project invariant, not a new file quietly exempting itself**: the two verbs
//! the spec needs are permitted, the rest stay forbidden, and the narrowing is
//! enforced by a test rather than by a comment. Without this file, the doctrine
//! would simply not apply to `spec/snapshot.rs`, since the existing walkers are
//! scoped to their own modules by filename.

use std::path::Path;

fn snapshot_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/spec/snapshot.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The verbs the existing walkers forbid, minus the two §4.4 requires.
///
/// Same spelling convention as `a206_workspace_identity.rs`: the quoted form,
/// so a verb inside prose or a doc comment is not a false positive and a verb
/// in an argv array is not a false negative.
const STILL_FORBIDDEN: &[&str] = &[
    "\"stash\"",
    "\"reset\"",
    "\"checkout\"",
    "\"clean\"",
    "\"push\"",
    "\"pull\"",
    "\"merge\"",
    "\"rebase\"",
    "\"restore\"",
    "\"rm\"",
    "\"mv\"",
    "\"apply\"",
    "\"init\"",
    "\"tag\"",
    "\"branch\"",
    "\"switch\"",
    "\"fetch\"",
    "\"clone\"",
    "\"gc\"",
    "\"update-ref\"",
    "\"update-index\"",
    "\"read-tree\"",
    "\"checkout-index\"",
    "\"filter-branch\"",
    "\"commit\"",
];

/// Exactly the two §4.4 permits.
const PERMITTED_MUTATING: &[&str] = &["\"add\"", "\"write-tree\""];

#[test]
fn the_snapshot_engine_uses_no_mutating_git_verb_beyond_the_two_the_spec_requires() {
    let src = snapshot_source();
    for verb in STILL_FORBIDDEN {
        assert!(
            !src.contains(verb),
            "spec/snapshot.rs invokes git {verb}. §4.4 permits `add` and \
             `write-tree` and nothing else; every other mutating verb stays \
             forbidden by the doctrine BUILD-PLAN C.6 narrowed rather than dropped."
        );
    }
}

#[test]
fn the_narrowing_is_real_and_not_a_list_that_matches_nothing() {
    // The negative control. A forbidden-verb walk over a module that happens to
    // contain none of the permitted verbs either would pass while proving
    // nothing — and would keep passing if the engine were rewritten to use
    // `git2`, or deleted. This asserts the exemption is actually being used.
    let src = snapshot_source();
    for verb in PERMITTED_MUTATING {
        assert!(
            src.contains(verb),
            "spec/snapshot.rs no longer invokes git {verb}. If the engine changed \
             approach, narrow this exemption again — an exemption nothing uses is \
             a hole left open for the next module."
        );
    }
}

#[test]
fn the_temporary_index_is_absolute_and_the_environment_is_cleared() {
    // Two properties the integration tests prove behaviourally, pinned here at
    // the source level too, because both are one-word edits away from being
    // silently lost and neither failure is loud at runtime:
    //
    //  - a relative GIT_INDEX_FILE is resolved by git against the repository
    //    root, so it can name `.git/index` itself;
    //  - an inherited GIT_INDEX_FILE (git sets one for several hooks, and
    //    ThruLine runs inside hooks) would otherwise be used by the read-only
    //    commands.
    let src = snapshot_source();
    assert!(
        src.contains("GIT_INDEX_FILE"),
        "the engine no longer sets a private index"
    );
    assert!(
        src.contains("env_remove(\"GIT_INDEX_FILE\")"),
        "the engine no longer clears an inherited GIT_INDEX_FILE"
    );
    assert!(
        src.contains("index_path.is_absolute()"),
        "the engine no longer asserts its temporary index path is absolute"
    );
}
