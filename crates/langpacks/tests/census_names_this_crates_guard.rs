//! The core's census names a test function in this crate. Keep the name true.
//!
//! # Why a rename here is silent, and expensive
//!
//! `crates/core/src/redact.rs` holds the conditional-truncation list, and two of
//! its entries — `measure.rs`'s `one_line` and `head_token` — carry
//!
//! ```text
//! guard: TruncationGuard::TestExists {
//!     file: "crates/langpacks/tests/measure_refuses_captured_input.rs",
//!     func: "a_corpus_under_a_captured_data_root_is_refused",
//! },
//! ```
//!
//! The census checks that the named function exists. So the **name** is part of
//! a cross-lane contract, and an ordinary tidy-up — renaming a test, splitting
//! a file — un-guards two truncation entries **silently**, in a crate whose
//! author has no reason to look at `redact.rs` that day. The entries would go
//! back to `UNGUARDED` and the census would say so; nobody in this lane would be
//! reading its output.
//!
//! This test closes that in **both** directions. It derives the names the core
//! actually asks for, from the core's own source, and asserts each exists here:
//!
//! - rename or move the function → this fails, naming the entry that would have
//!   been un-guarded;
//! - the core re-points a guard at a function this crate does not have → this
//!   fails too, before the census does.
//!
//! It is the same method as `detector_name_parity` and `capture_reachability`:
//! derive from the other lane's source rather than transcribe, so the claim
//! cannot rot into a comment that used to be true.

use std::path::PathBuf;

fn redact_rs() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../core/src/redact.rs")
        .canonicalize()
        .expect("crates/core/src/redact.rs — this test is about its contents");
    std::fs::read_to_string(&p).expect("read redact.rs")
}

/// Every `TestExists` guard the census points at this crate, as
/// `(file, func)` pairs read out of the core's source.
fn guards_pointing_here(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(at) = src[from..].find("TruncationGuard::TestExists") {
        let start = from + at;
        let block = &src[start..(start + 400).min(src.len())];
        let field = |name: &str| -> Option<String> {
            let k = format!("{name}: \"");
            let i = block.find(&k)? + k.len();
            let j = block[i..].find('"')?;
            Some(block[i..i + j].to_string())
        };
        if let (Some(f), Some(fun)) = (field("file"), field("func")) {
            if f.starts_with("crates/langpacks/") {
                out.push((f, fun));
            }
        }
        from = start + 1;
    }
    out
}

/// A guard on the derivation. If it parsed nothing, every assertion below would
/// pass vacuously — the "empty search space" failure this repository refuses for
/// its own deny list.
#[test]
fn the_derivation_finds_the_guards_the_census_points_here() {
    let found = guards_pointing_here(&redact_rs());
    assert!(
        !found.is_empty(),
        "no `TruncationGuard::TestExists` in crates/core/src/redact.rs points at \
         crates/langpacks. Either the census stopped guarding this crate's \
         truncations — in which case those entries are UNGUARDED and someone \
         should know — or this parse is broken."
    );
}

#[test]
fn every_function_the_census_names_here_exists() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (file, func) in guards_pointing_here(&redact_rs()) {
        // The census's path is repo-relative; this crate lives one level down.
        let rel = file
            .strip_prefix("crates/langpacks/")
            .expect("checked by the filter above");
        let path = root.join(rel);
        assert!(
            path.is_file(),
            "the census names `{file}` as a guard and it does not exist. Moving \
             or deleting it un-guards a truncation entry silently."
        );
        let text = std::fs::read_to_string(&path).expect("read the guard file");
        assert!(
            text.contains(&format!("fn {func}(")),
            "the census names `{func}` in `{file}` as the guard for a truncation \
             entry, and no such function is there.\n\n\
             If this test failed after a rename, the rename un-guarded that entry: \
             either restore the name, or re-point `redact.rs` in the same change \
             — the core's lane, so file it in docs/tracks/G/INTERFACE-REQUESTS.md \
             rather than editing it."
        );
    }
}
