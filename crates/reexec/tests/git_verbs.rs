//! The narrowed forbidden-git-verb guard for §7's materialiser.
//!
//! Same shape as `crates/core/tests/spec_snapshot_git_verbs.rs`, and for the
//! same reason: a doctrine enforced by a comment is a doctrine that lasts until
//! the next refactor. The standing ruling here (BUILD-PLAN C.7/4) is that
//! materialisation goes through `git cat-file` and **never** `git checkout` —
//! a checkout applies smudge filters, so its content is not a function of the
//! tree hash.
//!
//! `crates/reexec/tests/materialize.rs` demonstrates the *behavioural*
//! difference. This file makes the mechanism itself unreachable, which is the
//! control that keeps working when someone rewrites the module.

use std::path::Path;

fn source(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Every verb that would materialise content through a filter, plus the
/// mutating verbs the project-wide doctrine forbids.
///
/// Quoted, like the sibling walkers: a verb inside prose is not a false
/// positive and a verb in an argv array is not a false negative.
const FORBIDDEN: &[&str] = &[
    "\"checkout\"",
    "\"checkout-index\"",
    "\"read-tree\"",
    "\"restore\"",
    "\"archive\"",
    "\"worktree\"",
    "\"stash\"",
    "\"reset\"",
    "\"clean\"",
    "\"push\"",
    "\"pull\"",
    "\"merge\"",
    "\"rebase\"",
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
    "\"add\"",
    "\"write-tree\"",
    "\"commit\"",
    "\"filter-branch\"",
];

/// Exactly what §7 needs: read the objects, and list the tree.
const PERMITTED: &[&str] = &["\"cat-file\"", "\"ls-tree\""];

#[test]
fn the_materialiser_never_reaches_a_verb_that_would_apply_a_filter() {
    let src = source("src/materialize.rs");
    for verb in FORBIDDEN {
        assert!(
            !src.contains(verb),
            "reexec/materialize.rs invokes git {verb}. §7 materialises through \
             `cat-file` only: a checkout applies the SMUDGE half of any \
             `.gitattributes` filter pair, so its content is not a function of the \
             tree hash and can hold bytes that exist in no tree. That is the \
             laundering path BUILD-PLAN C.7/4 closed."
        );
    }
}

#[test]
fn the_permitted_verbs_are_actually_used_so_this_walk_is_not_vacuous() {
    // The negative control. A forbidden-verb walk over a module that reaches
    // git some other way would pass while proving nothing.
    let src = source("src/materialize.rs");
    for verb in PERMITTED {
        assert!(
            src.contains(verb),
            "reexec/materialize.rs no longer invokes git {verb}. If it changed approach, \
             re-narrow this guard — an exemption nothing uses is a hole for the next module."
        );
    }
}

#[test]
fn the_docker_backend_reaches_no_git_verb_at_all() {
    // The backend runs containers; it has no business touching the object
    // store. Keeping the two apart is what lets the materialiser's guard above
    // be the only place the rule has to hold.
    let src = source("src/docker.rs");
    assert!(
        !src.contains("Command::new(\"git\")"),
        "the docker backend spawns git directly, so the materialiser's guard no longer \
         covers every path from a tree hash to a directory"
    );
}

#[test]
fn the_run_path_turns_the_network_off_unless_the_repository_asked() {
    // §7 "Network off by default" and §0.8 "nothing leaves the machine unless
    // the user enables it", pinned at the source level because the default is
    // one word away from inverting and no test output would look different.
    let src = source("src/docker.rs");
    assert!(
        src.contains("\"--network\""),
        "the run path no longer passes --network, so it inherits Docker's default bridge"
    );
    assert!(
        src.contains("if prepared.network { \"host\" } else { \"none\" }"),
        "the network default is no longer `none` unless the repository asked for `host`"
    );
}
