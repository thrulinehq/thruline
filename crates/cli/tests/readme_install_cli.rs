//! T-7-1 rehearsal: the README's install command, run literally, replaces the
//! build every hook on the machine runs — and the README must say so.
//!
//! The rehearsal found it the hard way: `cargo install --path crates/cli
//! --locked` writes `~/.cargo/bin/thruline`, `thruline init` had written that
//! exact path into every hook, and nothing on the page said that installing a
//! candidate "to try it" puts it in front of the real sessions. Pointing HOME
//! somewhere else does not move where Cargo installs.
//!
//! Two halves, because a sentence in a README is only worth pinning if the
//! behaviour it describes is pinned too:
//!
//! 1. `init` writes the full path of the binary that ran it into every hook it
//!    registers, and into the context-server entry. That is what makes "the
//!    file your hooks run" a fact about `~/.cargo/bin/thruline` rather than a
//!    guess — and it is also why `--root` is a real way out: a binary installed
//!    elsewhere is a different path, and no hook names it.
//! 2. The page that tells a reader to run the plain `cargo install` says it,
//!    and shows the side-by-side form with `--root` whose test binary it then
//!    tells the reader to run by that same folder. That page was the README's
//!    quickstart and `## Install`; the public README installs from a release
//!    archive and builds with `cargo build`, so the `cargo install` command
//!    lives only in the book's install page (`docs/book/src/install.md`,
//!    `## Getting the binary`) and the guard follows it there. The README is
//!    still checked: if it ever shows the plain command again, it must say
//!    the same thing.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_thruline");

/// A documentation file, by its path from the repository root.
fn doc(file: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

const INSTALL_PAGE: &str = "docs/book/src/install.md";

/// The text of one `## ` section, heading included.
fn section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text
        .find(heading)
        .unwrap_or_else(|| panic!("no `{heading}` section"));
    let end = text[start + heading.len()..]
        .find("\n## ")
        .map(|at| start + heading.len() + at)
        .unwrap_or(text.len());
    &text[start..end]
}

/// Lines inside ```-fenced blocks, trailing `# comment` removed.
fn fenced_commands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            inside = !inside;
            continue;
        }
        if inside {
            out.push(line.split(" #").next().unwrap_or(line).trim().to_string());
        }
    }
    out
}

/// Prose with its line breaks folded, so a sentence wrapped across two lines
/// is still one sentence to search.
fn prose(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

const PLAIN: &str = "cargo install --path crates/cli --locked";
const SIDE_BY_SIDE: &str = "#trying-a-new-build-beside-the-one-you-use";

#[test]
fn init_writes_the_full_path_of_the_binary_that_ran_it_into_every_hook() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");
    let out = Command::new(BIN)
        .args(["--db", db.to_str().expect("utf-8")])
        .args([
            "init",
            "--yes",
            "--settings",
            settings.to_str().expect("utf-8"),
        ])
        .args(["--claude-json", claude_json.to_str().expect("utf-8")])
        .env("HOME", temp.path())
        .env("THRULINE_HOME", temp.path().join(".thruline"))
        .output()
        .expect("run thruline init");
    assert!(
        out.status.success(),
        "init failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let installed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).expect("settings")).expect("json");
    let hooks = installed["hooks"].as_object().expect("hooks object");
    let mut commands: Vec<String> = Vec::new();
    for entries in hooks.values() {
        for entry in entries.as_array().into_iter().flatten() {
            for hook in entry["hooks"].as_array().into_iter().flatten() {
                if let Some(command) = hook["command"].as_str() {
                    commands.push(command.to_string());
                }
            }
        }
    }
    // Positive control: a walk that found nothing would pass every assertion
    // below. The sensor pack alone is several events.
    assert!(
        commands.len() >= 8,
        "init registered too few hooks: {commands:?}"
    );
    let program = PathBuf::from(BIN);
    for command in &commands {
        let names_this_binary = command.starts_with(BIN)
            || command.starts_with(&format!("'{BIN}'"))
            || command.starts_with(&format!("\"{BIN}\""));
        assert!(
            names_this_binary,
            "a hook does not name the binary that ran init by its full path, so the install page's \
             `~/.cargo/bin/thruline` sentence would not follow: {command}"
        );
    }
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&claude_json).expect("claude.json"))
            .expect("json");
    assert_eq!(
        config["mcpServers"]["thruline"]["command"]
            .as_str()
            .map(PathBuf::from),
        Some(program),
        "the context server entry names a different binary: {config}"
    );
}

/// What a page that shows the plain install command has to say next to it.
fn assert_says_the_plain_install_replaces_what_the_hooks_run(label: &str, body: &str) {
    let words = prose(body);
    assert!(
        words.contains("`~/.cargo/bin/thruline`"),
        "{label} shows `{PLAIN}` without saying where it puts the binary"
    );
    assert!(
        words.contains("your hooks run"),
        "{label} does not say the installed file is the one the hooks run"
    );
    assert!(
        body.contains(SIDE_BY_SIDE),
        "{label} does not lead to the side-by-side form"
    );
}

#[test]
fn the_readme_says_the_plain_install_replaces_what_the_hooks_run_wherever_it_shows_it() {
    // The install page is where the plain command now lives, so it must show
    // it (a positive control: a page that stopped showing it would otherwise
    // pass every check below by having nothing to check) and say what it does.
    let page = doc(INSTALL_PAGE);
    let body = section(&page, "## Getting the binary");
    // The lead-in, before the side-by-side subsection that this one points at.
    let lead = body
        .split("\n### Trying a new build beside the one you use")
        .next()
        .unwrap_or(body);
    assert!(
        fenced_commands(lead).iter().any(|c| c == PLAIN),
        "`{INSTALL_PAGE}` no longer shows `{PLAIN}` — this guard is aimed at the wrong text"
    );
    assert_says_the_plain_install_replaces_what_the_hooks_run(INSTALL_PAGE, lead);

    // The README no longer shows it. If it ever does again, the same words
    // have to stand beside it, in whichever section it appears.
    let readme = doc("README.md");
    for chunk in readme.split("\n## ").skip(1) {
        if fenced_commands(chunk).iter().any(|c| c == PLAIN) {
            let heading = chunk.lines().next().unwrap_or("");
            assert_says_the_plain_install_replaces_what_the_hooks_run(
                &format!("README `## {heading}`"),
                chunk,
            );
        }
    }
}

#[test]
fn the_side_by_side_form_installs_elsewhere_and_runs_the_binary_it_installed() {
    let text = doc(INSTALL_PAGE);
    let start = text
        .find("### Trying a new build beside the one you use")
        .expect("the side-by-side section");
    let end = text[start + 4..]
        .find("\n##")
        .map(|at| start + 4 + at)
        .unwrap_or(text.len());
    let body = &text[start..end];
    let commands = fenced_commands(body);

    let install = commands
        .iter()
        .find(|c| c.starts_with(PLAIN) && c.contains("--root"))
        .unwrap_or_else(|| panic!("no `{PLAIN} --root <dir>` in: {commands:?}"));
    let dir = install
        .split_whitespace()
        .skip_while(|w| *w != "--root")
        .nth(1)
        .unwrap_or_else(|| panic!("`--root` has no folder: {install}"));
    assert_ne!(
        dir, "~/.cargo",
        "the side-by-side folder is the one it avoids"
    );

    // Cargo's `--root <dir>` puts the binary in `<dir>/bin`; the page has to
    // run THAT one, or the trial is the old build under a new name.
    let trial = format!("{dir}/bin/thruline");
    assert!(
        commands.iter().any(|c| c.starts_with(&trial)),
        "the section installs into {dir} and never runs {trial}: {commands:?}"
    );
    let words = prose(body);
    assert!(
        words.contains("`~/.cargo/bin/thruline`") && words.contains("as it was"),
        "the section does not say the hooks' binary is left alone: {words}"
    );
    assert!(
        words.contains(&format!("cargo uninstall --root {dir} thruline-cli")),
        "the section does not say how to remove the trial build: {words}"
    );
}

const BREW_LINE: &str = "brew install <tap-owner>/thruline/thruline";
const BREW_NOTE: &str = "the exact line is published at launch";

/// BUILD-6 (founder interim ruling FI-21): until the tap has an owner, the
/// Homebrew line is written with the literal placeholder `<tap-owner>` in every
/// place a person reads it, and the same file says the exact line is published
/// at launch. No other form of the line may appear in any of those files, and
/// neither may an owner the tap does not have.
///
/// The public README installs from a release archive and does not offer
/// Homebrew at all, so it is held to the stricter half of the rule: it gives
/// no Homebrew line, or gives the one line with its note. The guide
/// (`docs/THRULINE.md`) is where the line is stated, and must still state it;
/// that is this guard's positive control. The book's pages are held to the
/// same rule as the README.
#[test]
fn the_homebrew_line_is_the_one_line_in_the_readme_and_the_guide_with_its_note() {
    let mut files: Vec<(String, bool)> = vec![
        ("README.md".to_string(), false),
        ("docs/THRULINE.md".to_string(), true),
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut pages: Vec<String> = std::fs::read_dir(root.join("docs/book/src"))
        .expect("docs/book/src")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".md"))
        .collect();
    pages.sort();
    assert!(pages.len() >= 12, "the book has {} pages", pages.len());
    files.extend(
        pages
            .into_iter()
            .map(|name| (format!("docs/book/src/{name}"), false)),
    );
    for (file, must_give_it) in files {
        let text = doc(&file);
        let mut given = 0;
        for (at, _) in text.match_indices("brew install") {
            // "Homebrew install" in a sentence is the word, not the command.
            if text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric())
            {
                continue;
            }
            let rest = &text[at..];
            let end = rest.find(['\n', '`']).unwrap_or(rest.len());
            assert_eq!(
                rest[..end].trim_end(),
                BREW_LINE,
                "{file} gives a Homebrew line other than the one line"
            );
            given += 1;
        }
        if must_give_it {
            assert!(
                given > 0,
                "{file} no longer gives the Homebrew line — this guard is aimed at the wrong text"
            );
        }
        if given > 0 {
            assert!(
                prose(&text).contains(BREW_NOTE),
                "{file} gives the line without saying the exact line is published at launch"
            );
        }
    }
}
