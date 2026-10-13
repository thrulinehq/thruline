//! **[RULING R2 / BUILD-PLAN C.9.]** `crates/verify` must never depend on
//! `crates/explain`.
//!
//! §6.9's honest-failure markers live in `explain`. If a verifier could reach
//! them, prose could excuse missing evidence — the exploit C.9 closed. The
//! closure is the dependency direction, and a direction is only closed while
//! something checks it.
//!
//! Checked by reading the manifests rather than by running `cargo tree`,
//! because a test that shells out to cargo inside a cargo build is a test that
//! fails for reasons unrelated to what it asserts.

use std::path::{Path, PathBuf};

fn crates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .to_path_buf()
}

fn manifest(name: &str) -> String {
    let path = crates_dir().join(name).join("Cargo.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Every `thruline-*` crate a manifest names.
fn thruline_deps(manifest_text: &str) -> Vec<String> {
    manifest_text
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let name = l.split_whitespace().next()?;
            name.strip_prefix("thruline-").map(|_| name.to_string())
        })
        .collect()
}

#[test]
fn verify_does_not_depend_on_explain_by_any_path() {
    let deps = thruline_deps(&manifest("verify"));
    assert!(
        !deps.iter().any(|d| d == "thruline-explain"),
        "crates/verify depends on crates/explain — a verifier can now read \
         §6.9's honest-failure markers, and prose can excuse missing evidence: \
         {deps:?}"
    );
    // Stronger, and true today: `verify` depends on no ThruLine crate at all,
    // so `explain` is unreachable from it transitively as well as directly.
    assert!(
        deps.is_empty(),
        "crates/verify gained a ThruLine dependency ({deps:?}). That is not \
         forbidden in itself, but it opens a transitive path to `explain`; \
         either drop it, or replace this assertion with a real transitive walk."
    );
}

#[test]
fn explain_does_depend_on_verify_which_is_the_direction_that_is_allowed() {
    // The control. If this fails the test above is vacuous — two crates that
    // do not know about each other satisfy it for the wrong reason.
    let deps = thruline_deps(&manifest("explain"));
    assert!(
        deps.iter().any(|d| d == "thruline-verify"),
        "explain no longer depends on verify, so the direction assertion above \
         proves nothing: {deps:?}"
    );
}

#[test]
fn the_honest_failure_markers_appear_in_no_crate_but_explain() {
    // Belt and braces: the type boundary stops a verifier CALLING the
    // recogniser, and this stops one reimplementing it next door.
    let distinctive = ["\"was unable\"", "\"did not manage\"", "\"gave up\""];
    for crate_name in ["verify", "claims", "capture", "ledger", "attest", "reexec"] {
        let src = crates_dir().join(crate_name).join("src");
        if !src.is_dir() {
            continue;
        }
        let mut hits = Vec::new();
        walk(&src, &mut |path, text| {
            if distinctive.iter().all(|m| text.contains(m)) {
                hits.push(path.display().to_string());
            }
        });
        assert!(
            hits.is_empty(),
            "the §6.9 marker list was reimplemented outside `explain`, where a \
             verdict could reach it: {hits:?}"
        );
    }
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                f(&path, &text);
            }
        }
    }
}
