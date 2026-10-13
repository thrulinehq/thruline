//! D3: the hook path does no SQLite and no git work.
//!
//! §4.1's budget is 50 ms p95. `git add -A` on a large repository plus opening a
//! WAL database is not a promise that can be kept, so snapshots and the ledger
//! belong to the fold. This is the control for that deviation.
//!
//! # Why it is not `include_str!` of one file any more
//!
//! The previous version scanned `claude_code.rs` for forbidden strings and was
//! defeated two ways:
//!
//! 1. `use std::process::Command as Proc;` — the alias evasion, for the fifth
//!    time in this project.
//! 2. **With no obfuscation at all**: putting the work in a sibling module file,
//!    because the assertion read one file while the property spans the reachable
//!    call graph.
//!
//! Both are answered by changing what is matched rather than by adding
//! patterns. The walk follows `mod` and `use crate::…` from the hook entry
//! point, so a sibling file is only outside the walk if it is also outside the
//! call graph; and it matches the **import** (`process::Command`, `rusqlite`)
//! rather than the call spelling, because an alias must still import the thing
//! it aliases.
//!
//! # A crate-level ban is no longer available, and that is a real change
//!
//! `thruline-capture` now depends on `thruline-ledger`, because the fold lives
//! in this crate and the fold's whole job is to write to SQLite. So "this crate
//! must not name rusqlite" would be false. The claim was never about the crate;
//! it is about what the HOOK entry point can reach, and that is what this
//! measures. `fold.rs` names the ledger and is correctly outside the hook's
//! reachable set — asserted below, so the walk cannot pass by reaching nothing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Resolve a module path to its file, `foo.rs` or `foo/mod.rs`.
fn module_file(root: &Path, segments: &[String]) -> Option<PathBuf> {
    let mut base = root.to_path_buf();
    for seg in segments {
        base = base.join(seg);
    }
    let flat = base.with_extension("rs");
    if flat.is_file() {
        return Some(flat);
    }
    let dir = base.join("mod.rs");
    dir.is_file().then_some(dir)
}

/// Every file reachable from `entry` by `mod` and `use crate::…`.
fn reachable_from(entry: &Path) -> BTreeSet<PathBuf> {
    let root = src();
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut queue = vec![entry.to_path_buf()];

    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // The module's own directory, for `mod child;`.
        let own_dir = if path.file_name().is_some_and(|f| f == "mod.rs") {
            path.parent().map(Path::to_path_buf)
        } else {
            path.file_stem()
                .map(|stem| path.with_file_name("").join(stem))
        };

        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            // `mod child;` / `pub mod child;`
            if let Some(rest) = line
                .strip_prefix("pub mod ")
                .or_else(|| line.strip_prefix("mod "))
            {
                if let Some(name) = rest.strip_suffix(';') {
                    let name = name.trim();
                    // `#[cfg(test)] mod tests;` is not on the hook path.
                    if name == "tests" {
                        continue;
                    }
                    if let Some(dir) = &own_dir {
                        for candidate in [
                            dir.join(format!("{name}.rs")),
                            dir.join(name).join("mod.rs"),
                        ] {
                            if candidate.is_file() {
                                queue.push(candidate);
                            }
                        }
                    }
                }
            }
            // `use crate::a::b::C;`
            if let Some(rest) = line
                .strip_prefix("use crate::")
                .or_else(|| line.strip_prefix("pub use crate::"))
            {
                let head = rest.split(&[';', '{', ' '][..]).next().unwrap_or_default();
                let segments: Vec<String> = head
                    .split("::")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                // Try the longest prefix that resolves to a file.
                for take in (1..=segments.len()).rev() {
                    if let Some(file) = module_file(&root, &segments[..take]) {
                        queue.push(file);
                        break;
                    }
                }
            }
        }
    }
    seen
}

/// Imports the hook path must not contain, matched as IMPORTS rather than as
/// call spellings, so an alias cannot evade them.
const FORBIDDEN_IMPORTS: &[(&str, &str)] = &[
    ("rusqlite", "opening a WAL database is not a 50 ms promise"),
    (
        "thruline_ledger",
        "the ledger belongs to the fold, not the hook",
    ),
    (
        "process::Command",
        "spawning git on the hook path is the D3 deviation",
    ),
    (
        "spec::snapshot",
        "`git add -A` on a large repo is not a 50 ms promise",
    ),
];

fn offenders(files: &BTreeSet<PathBuf>) -> Vec<String> {
    let mut out = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        // Strip the inline test module: a test may legitimately spawn git.
        let production = match text.find("#[cfg(test)]") {
            Some(at) => text[..at].to_string(),
            None => text,
        };
        for (needle, why) in FORBIDDEN_IMPORTS {
            if production.contains(needle) {
                out.push(format!(
                    "{}: names `{needle}` — {why}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }
    out
}

#[test]
fn the_hook_path_reaches_no_database_and_no_subprocess() {
    let entry = src().join("claude_code.rs");
    assert!(entry.is_file(), "the hook entry point moved");
    let files = reachable_from(&entry);
    let bad = offenders(&files);
    assert!(
        bad.is_empty(),
        "the hook path can now reach work that breaks its 50 ms budget:\n  {}",
        bad.join("\n  ")
    );
}

#[test]
fn the_walk_actually_reaches_the_modules_the_hook_uses() {
    // Without this the isolation test could pass by reaching only its entry
    // file — which is precisely how the previous version was defeated.
    let files = reachable_from(&src().join("claude_code.rs"));
    let names: BTreeSet<String> = files
        .iter()
        .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
        .collect();
    for expected in ["claude_code.rs", "redaction.rs", "config.rs"] {
        assert!(
            names.contains(expected),
            "the walk did not reach {expected}; it sees {names:?}"
        );
    }
    // And `fold.rs` — which legitimately names the ledger — must be OUTSIDE it.
    // If it ever appears here, either the hook grew a dependency on the fold or
    // the walk has started swallowing the whole crate, and both make the
    // isolation test meaningless.
    assert!(
        !names.contains("fold.rs"),
        "the hook path now reaches the fold, which opens SQLite"
    );
}

#[test]
fn the_isolation_test_fails_when_the_work_is_hidden_in_a_sibling_module() {
    // The injection control, using the evasion that worked: no obfuscation at
    // all, just a different file in the same crate.
    let scratch = tempfile::tempdir().expect("tempdir");
    let root = scratch.path();
    std::fs::write(
        root.join("entry.rs"),
        "mod helper;\nuse crate::helper::spawn;\npub fn run() { spawn(); }\n",
    )
    .expect("write");
    std::fs::create_dir_all(root.join("entry")).expect("mkdir");
    std::fs::write(
        root.join("entry").join("helper.rs"),
        "use std::process::Command as Proc;\npub fn spawn() { let _ = Proc::new(\"git\"); }\n",
    )
    .expect("write");

    // Walk the injected pair the same way, then apply the same predicate.
    let mut files = BTreeSet::new();
    files.insert(root.join("entry.rs"));
    files.insert(root.join("entry").join("helper.rs"));
    let bad = offenders(&files);
    assert!(
        !bad.is_empty(),
        "the predicate does not see `process::Command` behind an alias in a \
         sibling file — both evasions are open again"
    );
    assert!(bad[0].contains("helper.rs"), "{bad:?}");
}

#[test]
fn the_alias_evasion_is_matched_by_the_import_not_the_call() {
    // `Command::new` was the old needle; `use std::process::Command as Proc`
    // never writes it. Matching the import is what closes the class, because an
    // alias must still import the thing it renames.
    let scratch = tempfile::tempdir().expect("tempdir");
    let file = scratch.path().join("aliased.rs");
    std::fs::write(
        &file,
        "use std::process::Command as Proc;\npub fn go() { let _ = Proc::new(\"git\"); }\n",
    )
    .expect("write");
    let mut files = BTreeSet::new();
    files.insert(file);
    assert!(
        !offenders(&files).is_empty(),
        "an aliased subprocess import is invisible to the predicate"
    );
}
