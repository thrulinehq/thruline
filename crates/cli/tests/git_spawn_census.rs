//! Every product `git` subprocess refuses system and user config, or is named.
//!
//! Track R's F17(b): a real `~/.gitconfig` with `core.autocrlf=true` moved a
//! receipt's `subject.gitTree` from `13f4b50…` to `f1cb474…`, with
//! `capture_degraded:false` and `content_filters:null`. `snapshot::run_git`
//! set `LC_ALL` and `LANG` and nothing else, so the engine's own idea of which
//! tree it was describing came from a file the engine never chose.
//!
//! Fixing the one function R demonstrated would leave the other twenty-five.
//! So the search space is DERIVED from the source on every run rather than
//! transcribed here: a `git` spawned from a file this census has never seen
//! fails it. That is the point — the list below is exemptions, not inventory.

use std::path::{Path, PathBuf};

/// Files whose `git` MUST be allowed to read the real configuration, each with
/// the reason it is not a hole.
const EXEMPT: &[(&str, &str)] = &[
    (
        "crates/core/src/spec/git_env.rs",
        "C.7's git environment digest exists to OBSERVE the user's real git \
         configuration and record it in the receipt. Isolating it would make it \
         report an empty config on every machine, which is the opposite of what \
         it is for — and would hide exactly the setting F17(b) exploited.",
    ),
    (
        "crates/cli/src/demo_cmd.rs",
        "The demo COMMITS, and takes its identity from the user's git config. \
         Isolating it without supplying GIT_AUTHOR_*/GIT_COMMITTER_* would break \
         `thruline demo` on every machine. It builds a throwaway repository and \
         binds no receipt, so it is not on F17(b)'s path. Giving it an explicit \
         identity and isolating it is worth doing and is not this change.",
    ),
];

/// Lanes this census does not police: another track owns the file.
const OTHER_LANES: &[&str] = &["crates/langpacks/", "crates/bench/"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

fn product_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("crates")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                // `tests/` holds test code; `target` is build output.
                if name == "tests" || name == "target" {
                    continue;
                }
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if name != "tests.rs" {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

fn spawns_git(line: &str) -> bool {
    [
        "Command::new(&git)",
        "Command::new(git)",
        "Command::new(\"git\")",
        "Command::new(&self.git)",
    ]
    .iter()
    .any(|p| line.contains(p))
}

#[test]
fn every_product_git_spawn_refuses_system_and_user_config() {
    let root = repo_root();
    let mut unhardened: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut exempted = 0usize;

    for path in product_sources(&root) {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if OTHER_LANES.iter().any(|l| rel.starts_with(l)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Stop at the file's own test module. `#[cfg(test)]` code is not the
        // product, and a unit test that shells out to `git init` is not a path
        // a receipt travels. This is a line-based approximation, not a parse:
        // it takes the FIRST `mod tests {` at column 0, which is this
        // codebase's convention throughout.
        let full: Vec<&str> = text.lines().collect();
        let end = full
            .iter()
            .position(|l| l.starts_with("mod tests {") || l.starts_with("#[cfg(test)]"))
            .unwrap_or(full.len());
        let lines: Vec<&str> = full[..end].to_vec();
        for (i, line) in lines.iter().enumerate() {
            if !spawns_git(line) {
                continue;
            }
            if let Some((_, _why)) = EXEMPT.iter().find(|(f, _)| *f == rel) {
                exempted += 1;
                continue;
            }
            checked += 1;
            // The isolation must be applied to THIS spawn: either inline in the
            // builder chain, or via the shared helper on the binding.
            let window = lines[i..lines.len().min(i + 14)].join("\n");
            let hardened =
                window.contains("GIT_CONFIG_NOSYSTEM") || window.contains("isolate_config");
            if !hardened {
                unhardened.push(format!("{rel}:{}", i + 1));
            }
        }
    }

    assert!(
        checked > 0,
        "the census found NO git spawns at all, which means its matcher stopped \
         matching rather than that the product stopped spawning git"
    );
    assert!(
        unhardened.is_empty(),
        "{} product git spawn(s) can be steered by config the engine did not \
         choose:\n  {}\n\n\
         SCOPE: F17(b). A `~/.gitconfig` with core.autocrlf=true moved a \
         receipt's subject.gitTree with capture_degraded:false — the engine's \
         idea of which tree it describes came from a file it never chose. Add \
         `crate::git_isolation::isolate_config(&mut cmd)` (or the two env vars \
         inline). If the spawn genuinely MUST read the real config, add it to \
         EXEMPT with the reason — a named exemption is the only allowed hole.\n\n\
         checked {checked}, exempt {exempted}",
        unhardened.len(),
        unhardened.join("\n  ")
    );

    // The exemptions are claims about files, so they must still exist.
    for (file, _) in EXEMPT {
        assert!(
            root.join(file).exists(),
            "EXEMPT names {file}, which is not in the tree — an exemption for a \
             file that moved is a hole nobody can see"
        );
    }
}
