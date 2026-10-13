//! Every file this crate's own prose cites must exist.
//!
//! # Why this is a test
//!
//! The Track Z coordinator read this crate at `04d86de` and found six citations
//! to files that were not there: MERGE-PLAN.md, MEASUREMENTS.toml,
//! tests/measured_constants.rs, and tests/honest_and_cheat_families.rs —
//! (named WITHOUT backticks on purpose: this matcher reads a backticked path as
//! a claim that the file exists, and the whole point is that they did not) —
//! the last of which never existed under that name at all, because the family
//! tests are `tests/families_<lang>.rs`. A reader was the thing that caught it.
//!
//! That is the same failure mode this whole crate is built to detect, committed
//! by the crate. A doc comment that names a gate is a claim that the gate
//! exists; if it does not, the comment is load-bearing prose that supports
//! nothing, and the only person who finds out is someone who goes looking.
//! Rule 12 of the project rules — never hide skipped work — and the `perf-basis.toml`
//! precedent, where a figure whose basis had moved was printed for three days
//! after it went false, both point the same way: the claim and its backing are
//! checked together or not at all.
//!
//! # What counts as a citation
//!
//! A backticked token that looks like a repo path: it contains a `/` and a file
//! extension, or it is a bare filename with one of the extensions this crate
//! actually references. Resolved against the crate root first, then the
//! workspace root. Prose, node kinds, argv fragments and code identifiers do not
//! match, which is checked by the guard test below — a matcher that found
//! nothing would make this file pass forever.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Extensions worth checking. Deliberately short: this is about *documents and
/// source files this crate points a reader at*, not about every string that
/// happens to contain a dot.
const EXTENSIONS: &[&str] = &[".rs", ".md", ".toml", ".py", ".json", ".xml", ".txt"];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(crate_root)
}

/// Files whose prose this test reads.
fn sources() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for sub in ["src", "tests"] {
        walk(&crate_root().join(sub), &mut out);
    }
    for f in [
        "Cargo.toml",
        "README.md",
        "MERGE-PLAN.md",
        "MEASUREMENTS.toml",
    ] {
        let p = crate_root().join(f);
        if p.is_file() {
            out.push(p);
        }
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Fixtures are data, not prose. A Java fixture mentioning a path is
            // not this crate making a claim.
            if path.file_name().is_some_and(|n| n == "fixtures") {
                continue;
            }
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "toml") {
            out.push(path);
        }
    }
}

/// Backticked tokens that look like repo paths.
fn cited_paths(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        let token = &after[..close];
        rest = &after[close + 1..];

        if token.is_empty() || token.len() > 120 || token.contains(char::is_whitespace) {
            continue;
        }
        if !EXTENSIONS.iter().any(|e| token.ends_with(e)) {
            continue;
        }
        // Not a path: a Rust path expression, a glob, a URL, a shell fragment,
        // or a documentation TEMPLATE — `target/surefire-reports/TEST-<class>.xml`
        // describes a shape, and no single file can satisfy it.
        if token.contains("::")
            || token.contains('*')
            || token.contains("://")
            || token.contains('<')
            || token.contains('>')
        {
            continue;
        }
        // A bare `foo.rs` inside prose is usually shorthand for a file in the
        // same directory and is too ambiguous to resolve; require a separator
        // unless it is one of the root documents this crate names by bare name.
        const BARE_ALLOWED: &[&str] = &[
            "MERGE-PLAN.md",
            "MEASUREMENTS.toml",
            "README.md",
            "Cargo.toml",
            "CLAUDE.md",
            "INTERFACE-REQUESTS.md",
        ];
        if !token.contains('/') && !BARE_ALLOWED.contains(&token) {
            continue;
        }
        out.insert(token.to_string());
    }
    out
}

/// Resolve a citation, trying the places a reader would look.
///
/// Four bases, in the order a reader would try them: the crate root, the
/// workspace root, `crates/` (for `crates/verify/...` cited from in here), and
/// the citing file's own directory — `src/shape.rs` saying `langs/ruby.rs`
/// means the module beside it.
///
/// Then a **suffix match** against real files under the crate, which is what
/// makes `runs/cheat-assertion_swallowed-proof-wrapped.txt` resolve from
/// `tests/families_go.rs` without the citation having to spell out
/// `tests/fixtures/families/go/`. This is deliberately the weakest rule and it
/// is still strict enough for the case that matters: a name for a file that
/// was never created suffix-matches nothing.
fn resolves(token: &str, citing_file: &Path) -> bool {
    let bases = [
        crate_root(),
        workspace_root(),
        crate_root().join(".."),
        citing_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(crate_root),
    ];
    if bases.iter().any(|b| b.join(token).exists()) {
        return true;
    }
    let mut all = Vec::new();
    walk_all(&crate_root(), &mut all);
    let needle = format!("/{token}");
    all.iter().any(|p| p.to_string_lossy().ends_with(&needle))
}

fn walk_all(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_all(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// A guard on the matcher. A citation extractor that found nothing would make
/// the real test below pass forever — the exact "empty search space" failure
/// the core's own commit hook refuses for its deny list.
#[test]
fn the_matcher_finds_citations_at_all() {
    let mut total = 0usize;
    for file in sources() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        total += cited_paths(&text).len();
    }
    assert!(
        total >= 15,
        "the citation matcher found only {total} cited paths across the crate. \
         This crate's prose cites `crates/verify/src/tamper.rs`, \
         `MEASUREMENTS.toml` and many more; a count this low means the matcher \
         is broken and every assertion below is vacuous."
    );
}

#[test]
fn every_path_this_crate_cites_exists() {
    let mut missing: Vec<(String, String)> = Vec::new();
    for file in sources() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let where_ = file
            .strip_prefix(crate_root())
            .unwrap_or(&file)
            .display()
            .to_string();
        for token in cited_paths(&text) {
            if !resolves(&token, &file) {
                missing.push((where_.clone(), token));
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "this crate's prose cites {} path(s) that do not exist:\n{}\n\n\
         A doc comment naming a gate is a claim that the gate exists. Either \
         create the file or stop citing it — a citation to nothing is prose \
         that supports nothing, and only a reader finds out.",
        missing.len(),
        missing
            .iter()
            .map(|(w, t)| format!("  {w}  cites  {t}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
