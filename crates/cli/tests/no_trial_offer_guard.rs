//! C31 (CTO ruling C31-A1-RESIDUAL, 2026-10-07): the licence guard. It fails
//! if the licence text, the READMEs, or any string ThruLine ships offers a
//! trial. v0.2 has none (C20): everything is free except approving a version,
//! which needs a Pro licence.
//!
//! # What it reads (the scope, exactly)
//!
//! Every surface below is something a person reads, so on these surfaces any
//! use of the word counts as an offer. Nothing on them has a reason to mention
//! a trial at all now.
//!
//! * `LICENSE.md`, whole file: the licence that ships in every archive and dmg.
//! * `README.md`, whole file: the repository README.
//! * `packaging/public-release/README.md`, whole file: the README published
//!   beside the binaries on the public release repository.
//! * `packaging/homebrew/Formula/thruline.rb`, whole file: the formula's
//!   description and caveats, which `brew` prints.
//! * `docs/book/src/**/*.md`: the book.
//! * `ui/dist/**`, every `.html`, `.js`, `.css`, `.json`, `.svg` and `.txt`
//!   file: the UI embedded into the binary (`crates/cli/build.rs`). Fonts are
//!   binary and are not read.
//! * `crates/cli/src/**/*.rs` and `crates/licence/src/**/*.rs`: every string
//!   literal (plain, byte and raw) and every outer doc comment (`///` and
//!   `/** */`), because clap prints a command's doc comments as its `--help`.
//!   Plain `//` and `/* */` comments and inner `//!` docs do not ship and are
//!   not read. (`no_trial_copy.rs` already reads these trees line by line,
//!   comments included; this guard is the one that holds the whole C31 scope.)
//! * `release/macos/lib.sh`, the body of `install_text()` only: the words of
//!   `INSTALL.txt` inside the dmg. The rest of that file is in the allowlist.
//!
//! **The site copy (thrulinehq.com) is not in this repository, and this test
//! cannot see it.** It lives in a separate, founder-locked repository; this
//! test does not reach outside the repository and does not claim the site is
//! clean.
//!
//! # The pattern
//!
//! `trial` at the start of a word, any case: "trial", "Trial", "trials" and
//! "trialling" match; "industrial" and `no_trial` do not (`_` is a word
//! character, as in a regex `\b`).
//!
//! # The allowlist
//!
//! [`ALLOWLIST`] names, with a reason, the paths in this repository that use
//! the word legitimately: bench "trial" runs, guards whose forbidden-word
//! lists name it, a vendor payload, design references and process history.
//! None of them is on a surface above. The test holds the list honest three
//! ways: every entry must exist, every entry must still contain the word (a
//! stale entry fails), and no entry may sit inside a surface the test reads,
//! so the allowlist can never hide part of a surface.

use std::path::{Path, PathBuf};

/// The surfaces this guard reads.
enum Surface {
    /// One file, every line.
    Whole(&'static str),
    /// Every file under a folder with one of these extensions, every line.
    Tree(&'static str, &'static [&'static str]),
    /// Every `.rs` file under a folder: string literals and outer doc comments.
    RustTree(&'static str),
    /// The body of one shell function in one file.
    ShellFunction(&'static str, &'static str),
}

const SURFACES: &[Surface] = &[
    Surface::Whole("LICENSE.md"),
    Surface::Whole("README.md"),
    Surface::Whole("packaging/public-release/README.md"),
    Surface::Whole("packaging/homebrew/Formula/thruline.rb"),
    Surface::Tree("docs/book/src", &["md"]),
    Surface::Tree("ui/dist", &["html", "js", "css", "json", "svg", "txt"]),
    Surface::RustTree("crates/cli/src"),
    Surface::RustTree("crates/licence/src"),
    Surface::ShellFunction("release/macos/lib.sh", "install_text"),
];

/// Paths that use the word and are not offers, each with its reason.
const ALLOWLIST: &[(&str, &str)] = &[
    ("crates/bench/src", "bench vocabulary: a \"trial\" is one run of a benchmark task"),
    ("fixtures/terminal-bench-holdout-v1.tsv", "bench fixture: trial columns count runs"),
    ("crates/cli/tests/m5_bench_cli.rs", "bench test: asserts on run (\"trial\") counts"),
    ("scripts/pilot-run-guard-test.sh", "bench pilot guard: \"trial\" runs of the pilot"),
    (
        "crates/licence-issue/fixtures/paddle-transaction_completed.json",
        "third-party payload shape: Paddle's own `trial_period` field name",
    ),
    ("crates/cli/tests/no_trial_offer_guard.rs", "this guard: its pattern and its own controls"),
    ("crates/cli/tests/no_trial_copy.rs", "guard: its forbidden-word list"),
    ("crates/cli/tests/no_trial_v011_cli.rs", "guard: asserts the trial is gone from the binary"),
    ("crates/licence/tests/no_trial_v011.rs", "guard: asserts the trial module and source are gone"),
    ("ui/src/workspace/no-trial.test.js", "guard: the UI census's forbidden-word list"),
    (
        "release/macos/check-release-binary-cases.sh",
        "release harness: stand-ins that print a trial field or open approving from a trial file, so the check can be shown to refuse them",
    ),
    (
        "release/macos/README.md",
        "release harness documentation: says what the check refuses, a trial among it",
    ),
];

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| panic!("no repository root above {}", env!("CARGO_MANIFEST_DIR")))
}

/* ── the pattern ─────────────────────────────────────────────────────── */

/// Whether `text` holds `trial` at the start of a word, any case.
fn offers_trial(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(at) = lower[from..].find("trial") {
        let i = from + at;
        let starts_word = i == 0 || {
            let b = bytes[i - 1];
            !(b.is_ascii_alphanumeric() || b == b'_')
        };
        if starts_word {
            return true;
        }
        from = i + 1;
    }
    false
}

/* ── the Rust reader: string literals and outer doc comments ─────────── */

/// The text a Rust source can show a person: `(line, text)` for every string
/// literal and every outer doc comment. Plain and inner-doc comments, char
/// literals and lifetimes are stepped over.
fn rust_user_text(src: &str) -> Vec<(usize, String)> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            line += 1;
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            let start = i;
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            let text: String = c[start..i].iter().collect();
            if text.starts_with("///") && !text.starts_with("////") {
                out.push((line, text[3..].to_string()));
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            let doc = c.get(i + 2) == Some(&'*') && c.get(i + 3) != Some(&'/');
            let (start_line, start) = (line, i + 2);
            let mut depth = 1;
            i += 2;
            while i < c.len() && depth > 0 {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    if c[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            if doc {
                out.push((start_line, c[start..i.min(c.len())].iter().collect()));
            }
        } else if ch == '"' {
            let (start_line, start) = (line, i + 1);
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\\' {
                    i += 1;
                }
                if c.get(i) == Some(&'\n') {
                    line += 1;
                }
                i += 1;
            }
            out.push((start_line, c[start..i.min(c.len())].iter().collect()));
            i += 1;
        } else if ch == '\'' {
            // A char literal ('x', '\'', '\u{1F600}') or a lifetime ('a).
            if c.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < c.len() && c[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if c.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if ident(ch) {
            // A token starts here (whole identifiers are consumed below). A
            // raw string opens with `r` or `br` and then `"` or `#`; a raw
            // identifier (`r#type`) has no quote after its `#`.
            let r = if ch == 'r' {
                Some(i)
            } else if ch == 'b' && c.get(i + 1) == Some(&'r') {
                Some(i + 1)
            } else {
                None
            };
            if let Some(r) = r {
                let mut j = r + 1;
                let mut hashes = 0;
                while c.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if c.get(j) == Some(&'"') {
                    let (start_line, start) = (line, j + 1);
                    j += 1;
                    loop {
                        match c.get(j) {
                            None => break,
                            Some('"') if (1..=hashes).all(|k| c.get(j + k) == Some(&'#')) => break,
                            Some('\n') => line += 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    out.push((start_line, c[start..j.min(c.len())].iter().collect()));
                    i = (j + 1 + hashes).min(c.len());
                    continue;
                }
            }
            if ch == 'b' && c.get(i + 1) == Some(&'"') {
                // A byte string: the `"` branch reads it.
                i += 1;
                continue;
            }
            while i < c.len() && ident(c[i]) {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

/* ── walking the surfaces ────────────────────────────────────────────── */

fn files_under(dir: &Path, exts: &[&str], found: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .path();
        if path.is_dir() {
            files_under(&path, exts, found);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| exts.contains(&e))
        {
            found.push(path);
        }
    }
}

fn read(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The lines of `text` from `name() {` to the first `}` at column 0.
fn shell_function(text: &str, name: &str) -> Vec<(usize, String)> {
    let head = format!("{name}() {{");
    let mut out = Vec::new();
    let mut inside = false;
    for (n, l) in text.lines().enumerate() {
        if !inside && l.starts_with(&head) {
            inside = true;
        }
        if inside {
            out.push((n + 1, l.to_string()));
            if l == "}" {
                break;
            }
        }
    }
    out
}

/// Every `(file, line, text)` the scope holds, and how many files it read.
fn census(root: &Path) -> (Vec<(String, usize, String)>, usize) {
    let mut texts = Vec::new();
    let mut files = 0;
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).display().to_string();
    for surface in SURFACES {
        match surface {
            Surface::Whole(p) => {
                let path = root.join(p);
                files += 1;
                for (n, l) in read(&path).lines().enumerate() {
                    texts.push((p.to_string(), n + 1, l.to_string()));
                }
            }
            Surface::Tree(p, exts) => {
                let mut found = Vec::new();
                files_under(&root.join(p), exts, &mut found);
                assert!(!found.is_empty(), "{p}: the census found no file to read");
                for path in found {
                    files += 1;
                    for (n, l) in read(&path).lines().enumerate() {
                        texts.push((rel(&path), n + 1, l.to_string()));
                    }
                }
            }
            Surface::RustTree(p) => {
                let mut found = Vec::new();
                files_under(&root.join(p), &["rs"], &mut found);
                assert!(!found.is_empty(), "{p}: the census found no file to read");
                for path in found {
                    files += 1;
                    for (n, t) in rust_user_text(&read(&path)) {
                        texts.push((rel(&path), n, t));
                    }
                }
            }
            Surface::ShellFunction(p, name) => {
                let lines = shell_function(&read(&root.join(p)), name);
                assert!(lines.len() > 3, "{p}: no function {name}() to read");
                files += 1;
                for (n, l) in lines {
                    texts.push((p.to_string(), n, l));
                }
            }
        }
    }
    (texts, files)
}

/* ── the guard ───────────────────────────────────────────────────────── */

#[test]
fn nothing_shipped_and_no_licence_text_offers_a_trial() {
    let root = repo();
    let (texts, files) = census(&root);
    // Not vacuous: the census reached every surface, and a real amount of text.
    assert!(files > 100, "the census read only {files} files");
    let literals = texts.iter().filter(|(f, _, _)| f.ends_with(".rs")).count();
    assert!(
        literals > 5_000,
        "the Rust reader found only {literals} strings and docs"
    );
    assert!(
        texts
            .iter()
            .any(|(f, _, _)| f.starts_with("ui/dist/assets/")),
        "the census did not reach the built UI"
    );
    assert!(
        texts
            .iter()
            .any(|(f, _, t)| f == "LICENSE.md" && t.contains("Permitted Purpose")),
        "the census did not reach the licence's grant"
    );

    let hits: Vec<String> = texts
        .iter()
        .filter(|(_, _, t)| offers_trial(t))
        .map(|(f, n, t)| {
            let t: String = t.trim().chars().take(160).collect();
            format!("{f}:{n}: {t}")
        })
        .collect();
    assert!(
        hits.is_empty(),
        "C20: v0.2 has no trial, and these shipped or licence texts still offer one:\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_allowlist_is_real_and_hides_no_surface() {
    let root = repo();
    for (path, reason) in ALLOWLIST {
        assert!(
            !reason.is_empty(),
            "{path}: an allowlist entry needs a reason"
        );
        let full = root.join(path);
        assert!(
            full.exists(),
            "allowlist entry {path} does not exist: remove it"
        );
        let mut files = Vec::new();
        if full.is_dir() {
            files_under(
                &full,
                &["rs", "tsv", "sh", "js", "json", "md", "html"],
                &mut files,
            );
        } else {
            files.push(full.clone());
        }
        assert!(
            files.iter().any(|f| read(f).lines().any(offers_trial)),
            "allowlist entry {path} no longer uses the word: it is stale, remove it"
        );
        for surface in SURFACES {
            let scanned = match surface {
                Surface::Whole(p) | Surface::Tree(p, _) | Surface::RustTree(p) => *p,
                // Only one function of that file is read; the file itself is
                // not a scanned path, so an entry naming it hides nothing.
                Surface::ShellFunction(..) => continue,
            };
            let inside = path == &scanned
                || path.starts_with(&format!("{scanned}/"))
                || scanned.starts_with(&format!("{path}/"));
            assert!(
                !inside,
                "allowlist entry {path} overlaps the scanned surface {scanned}"
            );
        }
    }
}

/* ── the guard's own controls: it can see what it is for ────────────── */

#[test]
fn the_pattern_matches_an_offer_and_not_a_homonym_inside_a_word() {
    assert!(offers_trial(
        "from us that covers your use, or during the free trial we offer."
    ));
    assert!(offers_trial("Start your 14-day Trial"));
    assert!(offers_trial("free-trial"));
    assert!(offers_trial("Trials end after 14 days"));
    assert!(!offers_trial("industrial"));
    assert!(!offers_trial("no_trial_copy"));
    assert!(!offers_trial(
        "Free. Approving a version needs a Pro licence."
    ));
}

#[test]
fn the_rust_reader_finds_strings_and_help_text_and_skips_plain_comments() {
    let src = r####"
/// Start a free trial of Pro.
//! module docs: trial
// a plain comment: trial
/* a block comment: trial */
/** a doc block: trial */
fn f<'a>(x: &'a str) -> char {
    let q = '"';
    let e = '\'';
    println!("Your trial ends {x}");
    let raw = r#"a "quoted" trial"#;
    let bytes = b"trial bytes";
    let braw = br##"raw "# trial"##;
    let industrial = "industrial";
    q
}
"####;
    let found = rust_user_text(src);
    let lines: Vec<usize> = found
        .iter()
        .filter(|(_, t)| offers_trial(t))
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(
        lines,
        vec![2, 6, 10, 11, 12, 13],
        "the reader saw: {found:?}"
    );
}

#[test]
fn the_shell_reader_reads_install_text_only() {
    let text = "before() {\n  echo trial\n}\ninstall_text() {\n  cat <<'EOF'\nOpen the dmg.\nEOF\n}\nafter() {\n  echo trial\n}\n";
    let lines = shell_function(text, "install_text");
    assert_eq!(lines.first().map(|l| l.0), Some(4));
    assert_eq!(lines.last().map(|l| l.1.as_str()), Some("}"));
    assert!(!lines.iter().any(|(_, l)| offers_trial(l)));
}
