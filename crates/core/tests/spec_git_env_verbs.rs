//! The forbidden-git-verb guard for `spec/git_env.rs`.
//!
//! Same shape as `spec_snapshot_git_verbs.rs`, and **stricter**: §4.4 permits
//! `add` and `write-tree` in the snapshot engine because a tree hash covering
//! untracked files cannot be produced without them. Nothing in the environment
//! reader needs a mutating verb at all, so none is permitted.

use std::path::Path;

fn source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/spec/git_env.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

const FORBIDDEN: &[&str] = &[
    "\"add\"",
    "\"write-tree\"",
    "\"commit\"",
    "\"checkout\"",
    "\"checkout-index\"",
    "\"read-tree\"",
    "\"restore\"",
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
    "\"filter-branch\"",
];

const PERMITTED: &[&str] = &["\"config\"", "\"rev-parse\"", "\"ls-files\""];

#[test]
fn the_environment_reader_mutates_nothing() {
    let src = source();
    for verb in FORBIDDEN {
        assert!(
            !src.contains(verb),
            "spec/git_env.rs invokes git {verb}. This module READS the conditions the \
             evidence was produced under; a mutating verb here would change them."
        );
    }
}

#[test]
fn the_read_only_verbs_are_actually_used() {
    // The negative control: a walk over a module that reaches git some other
    // way would pass while proving nothing.
    let src = source();
    for verb in PERMITTED {
        assert!(
            src.contains(verb),
            "spec/git_env.rs no longer invokes git {verb}; re-narrow this guard"
        );
    }
    assert!(
        src.contains("env_remove(\"GIT_INDEX_FILE\")"),
        "an inherited GIT_INDEX_FILE is no longer cleared — ThruLine runs inside hooks, \
         where git sets one"
    );
}
