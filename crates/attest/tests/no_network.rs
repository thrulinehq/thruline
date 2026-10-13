//! **Sigstore keyless is not built, and this is the proof rather than the
//! promise.**
//!
//! `CHANGELOG.md` says *"nothing here contacts Fulcio, Rekor or a TUF root,
//! and no code path could."* That was a reading of the dependency graph — an
//! assertion in a document whose whole standard is that assertions are weaker
//! than tests. This file makes it a mechanical control.
//!
//! # Why it matters that it is unbuilt rather than half-built
//!
//! §8.1 hedges the `sigstore` crate's own verification completeness, and
//! BUILD-PLAN M2.4 calls it a research spike. Doing keyless *properly* means
//! three verifications — DSSE over PAE against a Fulcio-issued certificate, a
//! chain to the Sigstore root **via TUF**, and a Rekor inclusion proof — each of
//! which fails **silently toward "verified"** when implemented halfway, in a
//! subsystem whose entire purpose is that it does not do that. Half-building it
//! and having it look done is the one outcome worse than not building it.
//!
//! So the honest state is: not built, said out loud, and enforced — if someone
//! adds a socket to this crate without also making the verification real, this
//! test fails and they have to say so.
//!
//! # Three halves, and one of them is behind the compiler
//!
//! 1. **No network dependency reaches this crate.** Its manifest names no HTTP
//!    or TLS crate. This is the load-bearing one: with no dependency, a
//!    `sigstore::` or `reqwest::` path does not compile, so the *compiler* is
//!    the first control and the manifest walk is what keeps it that way.
//! 2. **No source file opens a socket.** `std::net` needs no dependency at all,
//!    so this one catches what the manifest cannot. **Measured**: injecting a
//!    `TcpStream::connect` into `dsse.rs` makes it fail.
//! 3. **No source file names the services.** Deliberately a *second* line
//!    rather than an independent one — `sigstore::rekor::…` fails to compile
//!    before this test runs, as measured. What it does catch is a compiling
//!    stub: a `fulcio_chain_ok()` that returns `true` is a code path a reader
//!    believes in, and injecting exactly that makes this fail.

use std::path::Path;

fn crate_file(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn sources() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("reading {dir:?}: {e}"));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        if let Ok(text) = std::fs::read_to_string(&path) {
            // Production paths only. A `#[cfg(test)]` module legitimately
            // contains the forbidden strings as fixtures — `report.rs`'s badge
            // test asserts the SVG fetches nothing by naming `http://` — and a
            // walk that could not tell them apart would force those assertions
            // out of the file they are about.
            let production = match text.find("#[cfg(test)]") {
                Some(at) => text[..at].to_string(),
                None => text,
            };
            out.push((name, production));
        }
    }
    assert!(out.len() >= 5, "the source walk found only {:?}", out.len());
    out
}

#[test]
fn the_attest_crate_has_no_network_dependency() {
    let manifest = crate_file("Cargo.toml");
    // Every crate a Rust program would reach a network through. If one is added
    // deliberately, this list is where the decision gets written down.
    for forbidden in [
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
    ] {
        assert!(
            !manifest.contains(forbidden),
            "crates/attest/Cargo.toml depends on {forbidden:?}. If a network path is \
             being built here, say so in CHANGELOG.md in the same commit."
        );
    }

    // **`sigstore` was on that list and is narrowed, not deleted** (Track B's
    // guard table, DECISIONS #130). The bare substring fired on the NAME —
    // `"thruline-sigstore".contains("sigstore")` — so keeping it would have
    // meant deleting the guard to land the dependency, which is how a
    // supply-chain bar gets removed by the change it was written to catch.
    //
    // Every transport above stays forbidden. What is allowed is exactly one
    // sigstore-shaped dependency, by exact name, with default features OFF —
    // which is what keeps the transport out of the graph.
    // Over the DEPENDENCY sections only. The first version of this scanned the
    // whole manifest and fired on the crate's own `description`, which says
    // "Sigstore receipts" — a guard matching a spelling rather than a shape,
    // which is the day's most-repeated defect, caught here by itself.
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line.contains("dependencies");
            continue;
        }
        if !in_deps || line.starts_with('#') || !line.to_ascii_lowercase().contains("sigstore") {
            continue;
        }
        assert!(
            line.starts_with("thruline-sigstore ="),
            "crates/attest/Cargo.toml has a sigstore-shaped dependency that is not \
             thruline-sigstore: {line}"
        );
        assert!(
            line.contains("default-features = false"),
            "thruline-sigstore is depended on WITH default features. They pull the \
             transport, which is the thing every entry above forbids: {line}"
        );
    }

    // A substring scan over one manifest cannot see a transitive `tokio`, and
    // saying so is the difference between a guard and a comfort. The graph is
    // held by `crates/sigstore`'s own `tests/no_network.rs`, which travelled
    // with the snapshot, derives from `cargo tree`, and runs in this same gate.
}

#[test]
fn no_source_file_reaches_for_a_socket() {
    for (name, text) in sources() {
        for forbidden in [
            "TcpStream",
            "UdpSocket",
            "TcpListener",
            "std::net",
            "reqwest::",
            "ureq::",
            "http://",
            "https://",
        ] {
            // Doc comments legitimately cite URLs (the in-toto and DSSE
            // specification URIs are *identifiers*, not endpoints), so the walk
            // is over lines that are not comments.
            let offending: Vec<String> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.starts_with("//") && !l.starts_with("*"))
                // A URI constant is a NAME, not a destination: `PREDICATE_TYPE`
                // and `STATEMENT_TYPE` are compared as strings and never
                // fetched, and the SVG namespace is an XML identifier that no
                // renderer resolves. Both are exempted by their exact text, so
                // a new URI has to be exempted deliberately.
                .filter(|l| !(l.starts_with("pub const") || l.starts_with("const")))
                .map(|l| l.replace("http://www.w3.org/2000/svg", "<svg-namespace>"))
                .filter(|l| l.contains(forbidden))
                .collect();
            assert!(
                offending.is_empty(),
                "crates/attest/src/{name} reaches {forbidden:?}: {offending:?}"
            );
        }
    }
}

// `no_source_file_reaches_a_sigstore_client` was here and is **inverted**, not
// deleted (Track B's guard table, DECISIONS #130). Its own failure message set
// the bar for replacing it:
//
//   "Either keyless is real — in which case the certificate chain, the TUF root
//   and the Rekor inclusion proof are all verified and the documents must stop
//   saying it is unbuilt — or it is a stub, which is worse than nothing here."
//
// Keyless is real, so an absence assertion cannot be the guard any more. The
// presence assertion that replaced it is
// `crates/attest/tests/keyless_path.rs`, which drives `attest::verify`'s own
// keyless arm against B's production bundle fixture and requires each of the
// five axes to be reported separately — and a corrupted bundle to fail. It
// lives in its own file because it needs a receipt harness this
// source-scanning file does not have.

#[test]
fn the_trailers_rekor_slot_exists_and_nothing_in_the_workspace_fills_it() {
    // §8.1's trailer is `ThruLine-Receipt: <sha256>[ rekor:<uuid>]`. The slot is
    // implemented — a receipt uploaded to Rekor elsewhere can carry its uuid —
    // and **this build never produces one**, because nothing here talks to
    // Rekor. Asserting the slot works AND that no caller fills it is stronger
    // than asserting either alone: it says the capability is absent rather than
    // merely unused.
    let with = thruline_attest::trailer::line(b"payload", Some("abc-123"));
    assert!(with.contains(" rekor:abc-123"), "{with}");
    let without = thruline_attest::trailer::line(b"payload", None);
    assert!(!without.contains("rekor:"), "{without}");

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut call_sites = Vec::new();
    for crate_dir in [
        "cli", "core", "capture", "verify", "claims", "explain", "ledger",
    ] {
        let dir = root.join("crates").join(crate_dir).join("src");
        let Ok(walk) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in walk.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines().map(str::trim) {
                if line.contains("trailer::line(") {
                    call_sites.push(format!("{}: {line}", path.display()));
                }
            }
        }
    }
    assert!(
        !call_sites.is_empty(),
        "no caller of `trailer::line` was found, so this walk proves nothing"
    );
    for site in &call_sites {
        assert!(
            site.contains("None"),
            "a caller fills the trailer's Rekor slot: {site}. Nothing in this build talks \
             to Rekor, so a uuid here came from somewhere unverified."
        );
    }
}

/// Words that make a sentence about keyless a *denial* that it is built.
const DENIALS: &[&str] = &[
    "not built",
    "unbuilt",
    "is not implemented",
    "not implemented",
];

/// Does this document still **state** that Sigstore keyless is not built?
///
/// # A relationship, not a co-occurrence
///
/// This used to be `text.contains("Sigstore keyless") && text.contains("not
/// built")` — anywhere in the document, in either order. `CHANGELOG.md` is
/// long and "not built" appears in it more than once (the cloud
/// re-execution backend is also unbuilt), so the two halves were satisfied by
/// two unrelated sentences. Measured: deleting the subject fired the guard,
/// and **changing its sentence to "are fully built" did not** — the other
/// occurrence of the predicate kept the conjunction true.
///
/// So the subject and the denial must meet **on the same line**, and *every*
/// line naming the subject must carry one. A document that says it is unbuilt
/// in one place and built in another is not a document that says it is
/// unbuilt.
/// **The documents must agree with the CODE about keyless, not with a fixed
/// answer** (Track B's guard 4, DECISIONS #130).
///
/// This required every line naming "Sigstore keyless" to deny it was built.
/// That was right while it was unbuilt and became wrong the moment it shipped —
/// and it could not simply be inverted, because `CHANGELOG.md` carries **dated
/// entries** that correctly said "not built" at the time they were written.
/// Inverting the rule would have demanded rewriting those, and history is never
/// rewritten here to look aligned.
///
/// So the rule now reads the manifest and requires the documents to match it:
///
/// - keyless **built** (`crates/attest` depends on `thruline-sigstore`) → the
///   documents must carry at least one line saying so, and historical denials
///   are left exactly as written;
/// - keyless **unbuilt** → every line naming it must deny it, which is the
///   original discipline, unchanged.
///
/// A document that says nothing at all still fails, in both directions.
fn states_keyless_matches_the_code(text: &str, built: bool) -> Result<(), String> {
    let naming: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("Sigstore keyless"))
        .collect();
    if naming.is_empty() {
        return Err("the document does not name Sigstore keyless at all".to_string());
    }
    if !built {
        for line in &naming {
            let lower = line.to_ascii_lowercase();
            if !DENIALS.iter().any(|d| lower.contains(d)) {
                return Err(format!(
                    "this line names Sigstore keyless without denying it is built: {}",
                    line.trim()
                ));
            }
        }
        return Ok(());
    }
    let affirms = naming.iter().any(|l| {
        let lower = l.to_ascii_lowercase();
        !DENIALS.iter().any(|d| lower.contains(d))
    });
    if affirms {
        Ok(())
    } else {
        Err(
            "the code builds Sigstore keyless and every line about it still says \
             it is not built. Add the current statement; do NOT rewrite the dated \
             entries that were true when written."
                .to_string(),
        )
    }
}

/// Does this build actually carry the keyless path?
fn keyless_is_built() -> bool {
    crate_file("Cargo.toml")
        .lines()
        .any(|l| l.trim().starts_with("thruline-sigstore ="))
}

#[test]
fn the_documents_and_the_code_agree_that_keyless_is_unbuilt() {
    // The control that keeps the three tests above from being a fossil: if
    // someone builds keyless and deletes these guards, the documents must stop
    // claiming otherwise in the same commit. This asserts the claim is still
    // being made, so a build that removes the guards and leaves the prose fails
    // here.
    for (name, text) in documents() {
        if let Err(why) = states_keyless_matches_the_code(&text, keyless_is_built()) {
            panic!(
                "{name} no longer states that Sigstore keyless is not built, and the \
                 code still has no path that could reach Fulcio, Rekor or a TUF root \
                 — {why}"
            );
        }
    }
    // And `lib.rs` says it too, where a reader of the crate meets it — with
    // B's condition on the replacement: not another unqualified claim.
    let lib = crate_file("src/lib.rs");
    if keyless_is_built() {
        assert!(
            lib.contains("Sigstore keyless is implemented"),
            "the crate docs do not say keyless is built, and the manifest says it is"
        );
        for still_missing in ["Rekor", "TUF", "RFC 3161"] {
            assert!(
                lib.contains(still_missing),
                "the crate docs claim keyless without naming what is still not built \
                 ({still_missing}). Swapping one unqualified claim for another learns \
                 nothing from the failure this guard records."
            );
        }
    } else {
        assert!(
            lib.contains("Not implemented: Sigstore keyless"),
            "the crate docs no longer name the gap"
        );
    }
}

/// The guard is validated by reintroducing both defects it is against.
///
/// A document guard that only fires on **deletion** is half a guard: the way
/// this one was actually walked around was **contradiction**, which deletion
/// tests cannot see. Both mutations are derived from the real documents, so
/// this control cannot go stale when the prose is reworded.
#[test]
fn the_document_guard_fires_on_a_contradiction_and_not_only_on_a_deletion() {
    for (name, text) in documents() {
        assert!(
            states_keyless_matches_the_code(&text, keyless_is_built()).is_ok(),
            "{name}: the unmutated document must pass, or the mutations below prove \
             nothing"
        );
        // 1. Contradiction, and its DIRECTION follows the code. While keyless
        //    was unbuilt the walk-around was a document claiming it was built;
        //    now that it is built, the walk-around is the reverse — a document
        //    where every mention still denies it, which is what a stale
        //    CHANGELOG left alone would look like if nobody added the current
        //    line. The mutation is derived either way, so this control cannot
        //    go stale when the prose is reworded.
        let contradicted: String = if keyless_is_built() {
            // Strip the affirmation, leaving only the dated denials.
            text.lines()
                .filter(|l| {
                    !(l.contains("Sigstore keyless")
                        && !DENIALS.iter().any(|d| l.to_ascii_lowercase().contains(d)))
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            text.lines()
                .map(|l| {
                    if l.contains("Sigstore keyless") {
                        let mut out = l.to_string();
                        for d in DENIALS {
                            out = out.replace(d, "built");
                        }
                        out
                    } else {
                        l.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_ne!(contradicted, text, "{name}: the mutation changed nothing");
        assert!(
            states_keyless_matches_the_code(&contradicted, keyless_is_built()).is_err(),
            "{name}: the guard accepted a document that disagrees with the code"
        );
        // 2. Deletion, which the old form did catch. Kept, so the fix does not
        //    trade one blind spot for another.
        let deleted = text.replace("Sigstore keyless", "");
        assert!(
            states_keyless_matches_the_code(&deleted, keyless_is_built()).is_err(),
            "{name}: the guard accepted a document that stopped saying anything"
        );
    }
}

/// The document that has to stay honest about what is built.
fn documents() -> Vec<(&'static str, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    vec![(
        "CHANGELOG.md",
        std::fs::read_to_string(root.join("CHANGELOG.md")).expect("CHANGELOG.md"),
    )]
}
