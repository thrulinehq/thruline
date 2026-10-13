//! The docs are checked against the product, not against the last time someone
//! read them.
//!
//! §17's done-when for M6 is *"a new user on a fresh machine gets from install
//! to a PR check in under ten minutes following only the docs"*. That cannot be
//! asserted from here — it needs a fresh machine and a new user, and it is named
//! as unproven in `REPORT.md`. What **can** be asserted is the failure mode that
//! would make it impossible: documentation that names a command the binary does
//! not have, a policy key the schema does not accept, or a finding no verifier
//! emits.
//!
//! Every check below is a walk over real files with a positive control, because
//! a census that matches nothing passes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_thruline");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

/// The repo-root documents this track owns. Scoped BY NAME, deliberately:
/// `README.md` and `CHANGELOG.md` sit beside them and belong to the core, and a
/// directory walk would sweep them in and redden this suite over somebody
/// else's prose.
const OWNED_ROOT_DOCS: &[&str] = &["CONTRIBUTING.md", "SECURITY.md"];

/// The book, plus the root documents this track owns.
///
/// `CONTRIBUTING.md` was written telling contributors not to type a command
/// they had not run, and typed one — surviving only because this walk did not
/// read that file. A rule the checker cannot reach is a rule its author is
/// exempt from.
fn book() -> Vec<(String, String)> {
    let dir = root().join("docs/book/src");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("docs/book/src")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .map(|p| {
            (
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                std::fs::read_to_string(&p).expect("read"),
            )
        })
        .collect();
    for name in OWNED_ROOT_DOCS {
        let p = root().join(name);
        assert!(p.is_file(), "{name} is this track's and is missing");
        out.push((
            (*name).to_string(),
            std::fs::read_to_string(&p).expect("read"),
        ));
    }
    out.sort();
    assert!(out.len() >= 12, "the book has {} pages", out.len());
    out
}

/// The verbs `thruline --help` lists.
fn real_verbs() -> BTreeSet<String> {
    let out = Command::new(BIN).arg("--help").output().expect("--help");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut verbs = BTreeSet::new();
    let mut in_commands = false;
    for line in text.lines() {
        if line.trim_end().ends_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.trim().is_empty() || !line.starts_with(' ') {
                if line.trim_end().ends_with("Options:") {
                    break;
                }
                continue;
            }
            if let Some(word) = line.split_whitespace().next() {
                if word.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !word.is_empty() {
                    verbs.insert(word.to_string());
                }
            }
        }
    }
    assert!(verbs.contains("demo"), "the help walk found {verbs:?}");
    assert!(verbs.contains("policy"), "the help walk found {verbs:?}");
    verbs
}

/// Every `thruline <verb>` the book tells a reader to type must exist.
#[test]
fn every_command_the_docs_name_exists() {
    let verbs = real_verbs();
    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (page, text) in book() {
        for (i, line) in text.lines().enumerate() {
            let mut cursor = 0usize;
            while let Some(rel) = line[cursor..].find("thruline ") {
                let at = cursor + rel;
                cursor = at + "thruline ".len();
                // **Only an invocation.** `thruline` must start the line or
                // follow a backtick or a shell prompt; otherwise this matches
                // the crate name in `cargo install --git … thruline-cli`, which
                // is not a verb and cost this test a false positive on its
                // first run.
                let before = line[..at].trim_end_matches(' ');
                let is_invocation = before.is_empty()
                    || before.ends_with('`')
                    || before.ends_with('$')
                    || before.ends_with("&&");
                if !is_invocation {
                    continue;
                }
                let rest = &line[cursor..];
                let Some(word) = rest.split_whitespace().next() else {
                    continue;
                };
                // Skip flags, paths, placeholders and prose. `{` covers a
                // `format!` placeholder; `<` covers a doc placeholder.
                if word.starts_with('-')
                    || word.contains('/')
                    || word.starts_with('<')
                    || word.starts_with('{')
                {
                    continue;
                }
                let word = word.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
                if word.is_empty() || !word.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                    continue;
                }
                checked += 1;
                if !verbs.contains(word) {
                    missing.push(format!("{page}:{}: `thruline {word}`", i + 1));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "the docs tell a reader to run commands that do not exist:\n  {}",
        missing.join("\n  ")
    );
    assert!(checked > 15, "the walk only examined {checked} invocations");
}

/// Every policy key the reference documents must be one the schema accepts, and
/// every key the schema accepts must be documented. Both directions: an
/// undocumented key is a feature nobody can find, and a documented one that
/// does not exist is a policy file that will not load.
#[test]
fn the_policy_reference_and_the_schema_agree() {
    let text = book()
        .into_iter()
        .find(|(name, _)| name == "policy.md")
        .map(|(_, t)| t)
        .expect("policy.md");
    let schema: BTreeSet<String> = thruline_policy::SCHEMA
        .iter()
        .map(|s| s.to_string())
        .collect();
    // The reference's table rows are `| key | default | meaning |`.
    let documented: BTreeSet<String> = text
        .lines()
        .filter(|l| l.starts_with("| `"))
        .filter_map(|l| l.split('`').nth(1))
        .map(|k| k.replace("<kind>", "*"))
        .collect();
    assert!(documented.len() >= 12, "{documented:?}");

    let undocumented: Vec<&String> = schema
        .iter()
        .filter(|k| {
            // Container keys have no row of their own; their children do.
            !matches!(
                k.as_str(),
                "claims"
                    | "claims.*"
                    | "claims.*.required_when"
                    | "promises"
                    | "stop_gate"
                    | "reexec"
            ) && !documented.contains(*k)
        })
        .collect();
    assert!(
        undocumented.is_empty(),
        "policy keys the schema accepts and the reference does not document: {undocumented:?}"
    );
    let invented: Vec<&String> = documented.iter().filter(|k| !schema.contains(*k)).collect();
    assert!(
        invented.is_empty(),
        "the policy reference documents keys the schema will refuse: {invented:?}"
    );
}

/// Every finding the verifier reference names must be one a verifier can emit,
/// **and every finding a verifier can emit must be documented**.
///
/// Both directions now. The second one used to be skipped on the grounds that a
/// finding added in a hurry should not fail the build for want of a paragraph —
/// but a detector nobody documents is a detector nobody can act on, and the
/// reference is the only place a reader learns what a finding means.
///
/// # The emitted set is read from call sites, not from any string in the crate
///
/// This walk used to ask whether the crate's source contained `"<name>"`
/// anywhere. It does not distinguish a detector name from a string a detector
/// *searches for*, and it let `pytest_current_test`, `node_env` and `nodeenv`
/// stand in the reference as findings for three chapters. They are the
/// environment variables `test_environment_sniffing` looks for. No verifier has
/// ever emitted a finding by any of those names.
///
/// So the set is built by walking `Finding::new(` call sites with balanced
/// parentheses and taking the string literals inside — which is where a
/// detector name actually lives.
fn emitted_findings() -> BTreeSet<String> {
    // Line comments are stripped first. `tamper.rs` explains the
    // `skip_marker_added` severity rule with the literal
    // `@pytest.mark.skipif(sys.platform == "win32")` inside a comment, and both
    // its parentheses and its string would otherwise be read as part of the
    // call — which is how `win32` briefly became a documented finding.
    let mut source = String::new();
    for entry in walk_rs(&root().join("crates/verify/src")) {
        let text = std::fs::read_to_string(entry).expect("read");
        for line in text.lines() {
            let code = match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            };
            source.push_str(code);
            source.push('\n');
        }
    }
    let mut names = BTreeSet::new();
    let bytes: Vec<char> = source.chars().collect();
    let mut i = 0usize;
    let needle: Vec<char> = "Finding::new(".chars().collect();
    while i + needle.len() < bytes.len() {
        if bytes[i..i + needle.len()] != needle[..] {
            i += 1;
            continue;
        }
        // Balance parentheses from the opening one to find the whole call.
        let mut depth = 0usize;
        let mut j = i + needle.len() - 1;
        let start = j;
        while j < bytes.len() {
            match bytes[j] {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        let call: String = bytes[start..j.min(bytes.len())].iter().collect();
        // Detector names are snake_case literals. A `detail` is a sentence, so
        // it is excluded by the space and punctuation test below.
        for lit in call.split('"').skip(1).step_by(2) {
            if !lit.is_empty()
                && lit.len() <= 40
                && lit
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                names.insert(lit.to_string());
            }
        }
        i = j.max(i + 1);
    }
    // `{category}_failed` is built with `format!`, so the literal in the source
    // is the template. The categories are the ones `claims_cmd` dispatches.
    names.remove("{category}_failed");
    for c in ["build", "lint", "typecheck"] {
        names.insert(format!("{c}_failed"));
    }
    assert!(
        names.len() > 30,
        "the call-site walk found only {} detector names: {names:?}",
        names.len()
    );
    // Positive control: a name that is a *searched-for* string and never a
    // detector must NOT be in this set. This is the exact false positive the
    // old substring walk let through.
    for sniff in ["pytest_current_test", "node_env", "nodeenv"] {
        assert!(
            !names.contains(sniff),
            "`{sniff}` is a string the sniffing detector searches for, not a finding it emits — \
             the call-site walk has regressed into a substring walk"
        );
    }
    names
}

fn walk_rs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_rs(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}

#[test]
fn the_verifier_reference_and_the_detectors_agree() {
    let text = book()
        .into_iter()
        .find(|(name, _)| name == "verifiers.md")
        .map(|(_, t)| t)
        .expect("verifiers.md");
    // The reference introduces each finding as a bold code span at the start of
    // a line, and groups the sibling ones: **`a`, `b`, `c`** · severity · …
    let mut documented: BTreeSet<String> = BTreeSet::new();
    for line in text.lines().filter(|l| l.starts_with("**`")) {
        let head = line.trim_start_matches("**");
        let head = head.split("**").next().unwrap_or_default();
        for name in head.split('`').skip(1).step_by(2) {
            if name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                documented.insert(name.to_string());
            }
        }
    }
    assert!(
        documented.len() >= 30,
        "the reference walk found only {} findings: {documented:?}",
        documented.len()
    );

    let emitted = emitted_findings();
    let invented: Vec<&String> = documented.difference(&emitted).collect();
    assert!(
        invented.is_empty(),
        "the verifier reference names findings no verifier emits: {invented:?}"
    );
    let undocumented: Vec<&String> = emitted.difference(&documented).collect();
    assert!(
        undocumented.is_empty(),
        "verifiers emit these findings and the reference does not document them: {undocumented:?}"
    );
}

/// Every finding the reference documents carries a severity and a worked
/// example, because a name with no example is a name a reader cannot recognise
/// in output.
#[test]
fn every_documented_finding_has_a_severity_and_an_example() {
    let text = book()
        .into_iter()
        .find(|(name, _)| name == "verifiers.md")
        .map(|(_, t)| t)
        .expect("verifiers.md");
    let lines: Vec<&str> = text.lines().collect();
    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with("**`") {
            continue;
        }
        checked += 1;
        let name = line.split('`').nth(1).unwrap_or("?").to_string();
        if !["fail", "warn", "info"].iter().any(|s| line.contains(s)) {
            bad.push(format!("{name}: no severity on its heading line"));
        }
        // A worked example is a blockquote before the next finding heading.
        let has_example = lines[i + 1..]
            .iter()
            .take_while(|l| {
                !l.starts_with("**`") && !l.starts_with("### ") && !l.starts_with("## ")
            })
            .any(|l| l.starts_with("> "));
        if !has_example {
            bad.push(format!("{name}: no example line"));
        }
    }
    assert!(checked >= 30, "the walk examined only {checked} findings");
    assert!(
        bad.is_empty(),
        "findings documented without severity or example:\n  {}",
        bad.join("\n  ")
    );
}

/// The book must not promise an install path this repository has not published.
#[test]
fn the_docs_do_not_promise_an_unpublished_install() {
    for (page, text) in book() {
        for claim in [
            "brew install thruline",
            "npx thruline",
            "curl -fsSL https://thrulinehq.com",
        ] {
            if let Some(at) = text.find(claim) {
                // Naming it is fine; presenting it as working is not. The
                // paragraph must say it is not published yet.
                let window = &text[at.saturating_sub(400)..(at + 400).min(text.len())];
                assert!(
                    window.contains("not published") || window.contains("built in `packaging/`"),
                    "{page} presents `{claim}` as working, and no release exists"
                );
            }
        }
    }
}

/// `mdbook build` has never run in this repository (it is not installed here),
/// so the one thing that would silently break the book — a `SUMMARY.md` entry
/// pointing at a file that does not exist — is checked here instead.
///
/// Both directions, for the same reason the policy check goes both ways: a
/// missing target breaks the build, and a chapter nothing links to is a page
/// nobody can reach.
#[test]
fn every_summary_link_resolves_and_every_chapter_is_linked() {
    let dir = root().join("docs/book/src");
    let summary = std::fs::read_to_string(dir.join("SUMMARY.md")).expect("SUMMARY.md");
    let linked: BTreeSet<String> = summary
        .split("](./")
        .skip(1)
        .filter_map(|s| s.split(')').next())
        .map(str::to_string)
        .collect();
    assert!(linked.len() >= 10, "the walk found {} links", linked.len());

    let missing: Vec<&String> = linked.iter().filter(|l| !dir.join(l).is_file()).collect();
    assert!(
        missing.is_empty(),
        "SUMMARY.md links to chapters that do not exist, which fails `mdbook build`: \
         {missing:?}"
    );

    let present: BTreeSet<String> = std::fs::read_dir(&dir)
        .expect("src/")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".md") && n != "SUMMARY.md")
        .collect();
    let orphans: Vec<&String> = present.difference(&linked).collect();
    assert!(
        orphans.is_empty(),
        "these chapters exist and nothing links to them, so no reader can reach them: \
         {orphans:?}"
    );
    // Positive control: a link to a chapter that is not there is caught.
    assert!(!dir.join("no-such-chapter.md").is_file());
}

// ---------------------------------------------------------------------------
// The flag and subcommand census.
//
// `every_command_the_docs_name_exists` checks the VERB and stops. That let four
// invocations stand in the book, each of which fails the moment a reader types
// it:
//
//   thruline policy init            — `policy` has no subcommands; the flag is `--init`
//   thruline policy check           — same; the bare form is the check
//   thruline init --harness codex   — `init` has no `--harness`; that is `hook-install`
//   thruline verify <receipt> --offline
//                                   — `verify` takes no positional and no
//                                     `--offline`; the verb is `verify-receipt`
//
// Every one of those named a real verb, so the verb census passed them. A
// reader following the quickstart hit the first at step six.
// ---------------------------------------------------------------------------

/// One `thruline …` invocation found in prose, with where it was found.
#[derive(Debug)]
struct Invocation {
    where_: String,
    verb: String,
    args: Vec<String>,
}

/// Pull invocations out of fenced code blocks and inline code spans.
fn invocations_in(page: &str, text: &str) -> Vec<Invocation> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        // Either a bare command line inside a fence, or an inline `thruline …`.
        let candidates: Vec<&str> = if line.starts_with("thruline ") {
            vec![line]
        } else {
            line.split('`')
                .skip(1)
                .step_by(2)
                .filter(|s| s.starts_with("thruline "))
                .collect()
        };
        for cand in candidates {
            // Stop at a shell comment or a pipeline; neither is part of the
            // invocation being documented.
            let cand = cand.split('#').next().unwrap_or(cand);
            let cand = cand.split('|').next().unwrap_or(cand);
            let mut words = cand.split_whitespace();
            words.next(); // `thruline`
            let Some(verb) = words.next() else { continue };
            if verb.starts_with('-') {
                continue; // `thruline --version`
            }
            let mut args = Vec::new();
            for w in words {
                if w == "--" {
                    break; // everything after is the child's, not ours
                }
                args.push(w.to_string());
            }
            out.push(Invocation {
                where_: format!("{page}:{}", i + 1),
                verb: verb.to_string(),
                args,
            });
        }
    }
    out
}

fn help_for(path: &[String]) -> String {
    let out = Command::new(BIN)
        .args(path)
        .arg("--help")
        .output()
        .expect("--help");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The flags on a help page that take a value, e.g. `--out <OUT>`.
///
/// Without this the walk reads the VALUE as a positional argument, and reports
/// `thruline receipt --out path` as "this command takes no positional" — a
/// false accusation, which is the one failure this whole suite exists to avoid
/// committing itself.
fn value_flags(help: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in help.lines() {
        let t = line.trim_start();
        if !t.starts_with("--") && !t.starts_with('-') {
            continue;
        }
        for part in t.split(", ") {
            let part = part.trim();
            let mut words = part.split_whitespace();
            let Some(flag) = words.next() else { continue };
            if !flag.starts_with("--") {
                continue;
            }
            if words.next().is_some_and(|w| w.starts_with('<')) {
                out.insert(flag.to_string());
            }
        }
    }
    out
}

/// The subcommands a help page lists, and whether it accepts a positional.
fn accepts(help: &str) -> (BTreeSet<String>, bool) {
    let mut subs = BTreeSet::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.trim_end().ends_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            if !line.starts_with(' ') || line.trim().is_empty() {
                in_commands = false;
                continue;
            }
            if let Some(w) = line.split_whitespace().next() {
                if w.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                    subs.insert(w.to_string());
                }
            }
        }
    }
    let positional = help
        .lines()
        .find(|l| l.trim_start().starts_with("Usage:"))
        .is_some_and(|u| {
            let after = u.split("[OPTIONS]").last().unwrap_or(u);
            after.contains('<')
        });
    (subs, positional)
}

/// Every flag and every subcommand the docs type must be one the binary parses.
#[test]
fn every_flag_and_subcommand_the_docs_type_is_real() {
    let verbs = real_verbs();
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for (page, text) in book() {
        for inv in invocations_in(&page, &text) {
            if !verbs.contains(&inv.verb) {
                continue; // the verb census owns that failure
            }
            let mut path = vec![inv.verb.clone()];
            let mut help = help_for(&path);
            let (mut subs, mut positional) = accepts(&help);
            let mut takes_value = value_flags(&help);
            let mut skip_next = false;
            for arg in &inv.args {
                if skip_next {
                    skip_next = false;
                    continue; // the value of the flag before it
                }
                checked += 1;
                if let Some(flag) = arg.split('=').next().filter(|a| a.starts_with("--")) {
                    if !help.contains(flag) {
                        bad.push(format!(
                            "{}: `thruline {} {flag}` — no such flag",
                            inv.where_,
                            path.join(" ")
                        ));
                    } else if takes_value.contains(flag) && !arg.contains('=') {
                        skip_next = true;
                    }
                    continue;
                }
                if arg.starts_with('-') {
                    continue; // short flags; clap prints them differently
                }
                // A bare word: a subcommand, or a positional value.
                if subs.contains(arg) {
                    path.push(arg.clone());
                    help = help_for(&path);
                    let (s, p) = accepts(&help);
                    subs = s;
                    positional = p;
                    takes_value = value_flags(&help);
                    continue;
                }
                if positional {
                    continue; // it is a value for the positional
                }
                if arg.starts_with('<') || arg.contains('/') || arg.contains('.') {
                    // A placeholder or a path where none is accepted is still
                    // wrong, and the message says which.
                    bad.push(format!(
                        "{}: `thruline {} {arg}` — this command takes no positional argument",
                        inv.where_,
                        path.join(" ")
                    ));
                    continue;
                }
                bad.push(format!(
                    "{}: `thruline {} {arg}` — not a subcommand of `{}` and it takes no positional \
                     (subcommands: {})",
                    inv.where_,
                    path.join(" "),
                    path.join(" "),
                    if subs.is_empty() {
                        "none".to_string()
                    } else {
                        subs.iter().cloned().collect::<Vec<_>>().join(", ")
                    }
                ));
            }
        }
    }

    assert!(
        checked > 25,
        "the walk examined only {checked} arguments, so it is not looking at the book"
    );
    assert!(
        bad.is_empty(),
        "the docs type arguments the binary will refuse:\n  {}",
        bad.join("\n  ")
    );
}

/// The same census, over the product's own output.
///
/// `thruline policy --init` printed *"Run `thruline policy check` to evaluate
/// it"* — a command that does not parse — and the GitHub check run told every
/// reviewer to run `thruline policy init`. A binary that recommends a command
/// it does not have is the docs failure with a shorter feedback loop, on a more
/// public surface.
///
/// Scoped to string literals: a doc comment is written for a maintainer reading
/// the source, and holding it to this rule would fail the build over a sentence
/// nobody types.
#[test]
fn every_command_the_product_recommends_exists() {
    let verbs = real_verbs();
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let dirs = [
        root().join("crates/cli/src"),
        root().join("crates/app-core/src"),
    ];
    for dir in dirs {
        for file in walk_rs(&dir) {
            let text = std::fs::read_to_string(&file).expect("read");
            let name = file
                .strip_prefix(root())
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            for (i, line) in text.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                // A recommendation lives inside a string literal, as an
                // escaped-backtick span: "... `thruline foo bar` ...".
                for span in line.split('`').skip(1).step_by(2) {
                    if !span.starts_with("thruline ") {
                        continue;
                    }
                    // Only inside a string literal, not a doc comment.
                    if !line.contains('"') {
                        continue;
                    }
                    let mut words = span.split_whitespace();
                    words.next();
                    let Some(verb) = words.next() else { continue };
                    // `{verb}`, `{cmd}` — a `format!` placeholder is not a verb.
                    // `uiserve.rs` builds an error message as
                    // `format!("running `thruline {verb} --json`")`, and reading
                    // its placeholder as a command name is the census accusing a
                    // template of naming something that does not exist. The
                    // interpolated value is checked where it is CONSTRUCTED, not
                    // here, because here it is not a string yet.
                    if verb.starts_with('-') || verb.starts_with('<') || verb.starts_with('{') {
                        continue;
                    }
                    checked += 1;
                    if !verbs.contains(verb) {
                        bad.push(format!("{name}:{}: `{span}` — no such verb", i + 1));
                        continue;
                    }
                    let mut path = vec![verb.to_string()];
                    let mut help = help_for(&path);
                    let (mut subs, mut positional) = accepts(&help);
                    let mut takes_value = value_flags(&help);
                    let mut skip_next = false;
                    for arg in words {
                        if arg == "--" || arg.starts_with('<') {
                            break;
                        }
                        if skip_next {
                            skip_next = false;
                            continue;
                        }
                        if let Some(flag) = arg.split('=').next().filter(|a| a.starts_with("--")) {
                            if !help.contains(flag) {
                                bad.push(format!(
                                    "{name}:{}: `{span}` — `{flag}` is not a flag of `{}`",
                                    i + 1,
                                    path.join(" ")
                                ));
                            } else if takes_value.contains(flag) && !arg.contains('=') {
                                skip_next = true;
                            }
                            continue;
                        }
                        if arg.starts_with('-') {
                            continue;
                        }
                        if subs.contains(arg) {
                            path.push(arg.to_string());
                            help = help_for(&path);
                            let (s, p) = accepts(&help);
                            subs = s;
                            positional = p;
                            takes_value = value_flags(&help);
                            continue;
                        }
                        if !positional {
                            bad.push(format!(
                                "{name}:{}: `{span}` — `{arg}` is not a subcommand of `{}` and it \
                                 takes no positional",
                                i + 1,
                                path.join(" ")
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        checked > 10,
        "the walk examined only {checked} recommendations"
    );
    assert!(
        bad.is_empty(),
        "the product recommends commands it does not have:\n  {}",
        bad.join("\n  ")
    );
}

/// No page may name a repository that does not exist.
///
/// # This guard is one of a PAIR, and neither is total coverage
///
/// `crates/cli/tests/m6_release_paths.rs` (Track C's) asserts the same string
/// is absent from the **packaging** paths — the installer, the npm shim, the
/// Homebrew formula, the Action. Its scope stops at `docs/book/`, correctly:
/// those files are this track's.
///
/// The hazard is the pair, not either half. Q measured C's guard green on a
/// tree that still carried three live occurrences, because the only test whose
/// failure message names `thruline/thruline` was not looking at the book. This
/// test is the other half. **A green here says the book is clean and says
/// nothing about packaging; a green there says the reverse.** Neither may be
/// read as "the string is gone".
///
/// `github.com/thruline/thruline` is not a repository — `git ls-remote` answers
/// *Repository not found* — and it stood in `quickstart.md` and `book.toml` as
/// a working install source and as the book's own repository link. Track Q
/// reproduced it as two separate blockers.
///
/// The check is a substring walk and not a network call, deliberately: a test
/// that resolves URLs fails offline and in CI without egress, and would be
/// disabled the first time it did. What it can hold is that a **known-absent**
/// address never comes back, which is the regression that actually happened.
#[test]
fn no_page_names_a_repository_that_does_not_exist() {
    // Known absent, verified by hand: `git ls-remote https://github.com/thruline/thruline`
    // → `remote: Repository not found.` (2026-09-05, and again 2026-09-06).
    // Both the URL form and the bare `owner/repo` form a GitHub Actions
    // `uses:` line takes. The URL-only version of this list was DECORATIVE for
    // the case it was written against: `- uses: thruline/action@v1` contains no
    // `github.com/`, so the walk passed the very line Track Q had flagged.
    const ABSENT: &[&str] = &[
        "github.com/thruline/thruline",
        "github.com/thruline/action",
        "thruline/action@",
        "thruline/thruline ",
    ];
    let mut named: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (page, text) in book() {
        // Inside a fenced block a disclaimer is not enough. Track Q's framing:
        // a fix that leaves a copy-pasteable line pointing at an organisation
        // that does not resolve "looks complete and is not". A reader copies the
        // block; the paragraph above it does not travel with them.
        let mut fenced = false;
        for (i, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            for bad in ABSENT {
                checked += 1;
                if !line.contains(bad) {
                    continue;
                }
                if fenced {
                    named.push(format!(
                        "{page}:{}: names `{bad}` inside a code block, where a reader copies the line and not the caveat above it",
                        i + 1
                    ));
                    continue;
                }
                // Naming one as unavailable is the whole point of some pages.
                // Presenting it as usable is not: a line that names it must also
                // say, within three lines, that it does not resolve.
                let window: String = text
                    .lines()
                    .skip(i.saturating_sub(3))
                    .take(7)
                    .collect::<Vec<_>>()
                    .join(" ");
                let disclaimed = [
                    "does not exist",
                    "does not resolve",
                    "not published",
                    "NOT YET RUNNABLE",
                    "Repository not found",
                ]
                .iter()
                .any(|d| window.contains(d));
                if !disclaimed {
                    named.push(format!("{page}:{}: names `{bad}` as if it worked", i + 1));
                }
            }
        }
    }
    assert!(checked > 100, "the walk examined only {checked} line-pairs");
    assert!(
        named.is_empty(),
        "pages present a repository that does not exist:\n  {}",
        named.join("\n  ")
    );
    // Positive control: the pattern this is written against is a real string.
    assert!(ABSENT[0].contains("thruline/thruline"));
}

/// `book.toml` must not point the book's repository link at a repository the
/// reader cannot reach — private, or absent.
///
/// It checked only the PRIVATE case at first, and that was D27's shape in this
/// track's own lane: of the three surviving `thruline/thruline` occurrences,
/// this file's was the one no guard here would have caught, because the walk in
/// `no_page_names_a_repository_that_does_not_exist` reads `docs/book/src/*.md`
/// and `book.toml` is neither a page nor markdown. Witnessed 2026-09-06 by
/// restoring all three defects: two reddened, this one passed.
#[test]
fn the_books_repository_link_is_reachable_or_unset() {
    let toml = std::fs::read_to_string(root().join("docs/book/book.toml")).expect("book.toml");
    let active: Vec<&str> = toml
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter(|l| l.starts_with("git-repository-url"))
        .collect();
    let bad: Vec<&&str> = active
        .iter()
        .filter(|l| {
            // Private: 404s for a reader without access. Absent: 404s for
            // everyone. mdBook renders this link on EVERY page, so either one
            // is a broken link the whole book carries.
            l.contains("example/thruline-private") || l.contains("thruline/thruline")
        })
        .collect();
    assert!(
        bad.is_empty(),
        "book.toml points every page at a repository the reader cannot reach \
         (private, or absent): {bad:?}"
    );
}
