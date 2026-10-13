//! The offline guarantee, proved from the dependency graph rather than asserted.
//!
//! `~/thruline/crates/attest/tests/no_network.rs:77` forbids a list of crate
//! names in `crates/attest/Cargo.toml`. That guard is good and it is also
//! **structurally unable to see a transitive dependency** — it is a substring
//! scan of one manifest, so a `tokio` arriving three levels down through an
//! innocuous-looking crate would pass it.
//!
//! `attest` is about to depend on this crate, so the property it needs is
//! *"nothing in `thruline-sigstore`'s default feature set can open a socket"*,
//! and that is a claim about the whole resolved graph. This test asks
//! `cargo tree` and fails on any transport it finds.
//!
//! It also asserts the opposite for the `net` feature — that enabling it
//! **does** pull a transport. A guard that only checks one direction cannot
//! tell "the feature gate works" from "the feature is broken and compiles to
//! nothing".

use std::process::Command;

/// Every crate a Rust program could reach a network through. The list is
/// `attest`'s own, so the two guards cannot drift apart silently.
const TRANSPORTS: &[&str] = &[
    "reqwest",
    "ureq",
    "hyper",
    "curl",
    "isahc",
    "surf",
    "attohttpc",
    "rustls",
    "native-tls",
    "openssl",
    "tokio",
    "tough",
    "reqwest-middleware",
];

fn tree(args: &[&str]) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args(["tree", "-e", "normal", "--prefix", "none"])
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    v.sort();
    v.dedup();
    v
}

fn names(tree: &[String]) -> Vec<String> {
    tree.iter()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

#[test]
fn the_default_feature_set_resolves_no_transport() {
    let crates = names(&tree(&[]));
    assert!(
        crates.len() > 40,
        "the walk found only {} crates, so it is not looking at a real graph",
        crates.len()
    );
    let found: Vec<&String> = crates
        .iter()
        .filter(|c| TRANSPORTS.contains(&c.as_str()))
        .collect();
    assert!(
        found.is_empty(),
        "the default feature set pulls {found:?}. `crates/attest` depends on this crate to \
         stay offline, and its own guard cannot see transitive dependencies — so this is the \
         test that has to catch it."
    );
}

/// A single `sha2`, matching `~/thruline/Cargo.toml`'s `sha2 = "0.11.0"`.
///
/// This is the measured reason `sigstore-merkle` was not adopted: it would have
/// brought `sha2 0.10.9` and `digest 0.10.7` in beside the 0.11 generation.
/// Two versions of a hash trait in one graph is the exact failure the
/// crypto-stack probe flagged as most likely to break the build.
#[test]
fn there_is_exactly_one_sha2_and_one_digest() {
    let lines = tree(&[]);
    for pkg in ["sha2", "digest"] {
        // `cargo tree` marks a repeated subtree with a trailing `(*)`, so the
        // identity of a package is its name AND version, nothing more.
        let distinct: std::collections::BTreeSet<String> = lines
            .iter()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let n = it.next()?;
                let v = it.next()?;
                (n == pkg).then(|| format!("{n} {v}"))
            })
            .collect();
        assert_eq!(
            distinct.len(),
            1,
            "{pkg} resolves to {distinct:?}; the workspace pins one and a second generation \
             breaks the trait bounds"
        );
    }
}

/// The other direction: the gate is real, not a no-op.
#[test]
fn enabling_net_does_pull_a_transport() {
    let crates = names(&tree(&["--features", "net"]));
    assert!(
        crates.iter().any(|c| c == "ureq"),
        "the `net` feature resolved no HTTP client, so either the gate is broken or the \
         feature compiles to nothing — and the offline test above would then be proving \
         nothing either"
    );
}

/// `ureq` is the project's convention (`CLAUDE.md`: *"HTTP: `ureq` (blocking,
/// simple) … No tokio until the watcher daemon milestone"*), so even the
/// networked build must not drag an async runtime in.
#[test]
fn even_the_networked_build_has_no_async_runtime() {
    let crates = names(&tree(&["--features", "net,oidc"]));
    assert!(
        !crates.iter().any(|c| c == "tokio"),
        "the networked build pulls tokio; CLAUDE.md defers an async runtime to the watcher \
         daemon milestone, and a blocking client is why `ureq` was chosen"
    );
}

/// No source file reaches for a socket outside the `net` module.
#[test]
fn no_source_file_outside_net_names_a_transport() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0;
    let mut offences = Vec::new();
    walk(&src, &mut |path: &std::path::Path| {
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            return;
        }
        // `src/net/` is where a transport belongs.
        if path.components().any(|c| c.as_os_str() == "net") {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        checked += 1;
        // Doc comments legitimately cite URLs as identifiers, and `const`
        // declarations hold well-known endpoint strings that nothing fetches.
        for line in text.lines().map(str::trim) {
            if line.starts_with("//") || line.starts_with("*") || line.starts_with("pub const") {
                continue;
            }
            for bad in ["ureq::", "TcpStream", "std::net", "reqwest::"] {
                if line.contains(bad) {
                    offences.push(format!("{}: {line}", path.display()));
                }
            }
        }
    });
    assert!(checked >= 8, "the source walk only saw {checked} files");
    assert!(offences.is_empty(), "{offences:#?}");
}

fn walk(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, f);
        } else {
            f(&p);
        }
    }
}
