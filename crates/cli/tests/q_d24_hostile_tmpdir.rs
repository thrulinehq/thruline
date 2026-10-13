//! Q's D24 — five guards searched the absolute path they were printing.
//!
//! # The measurement, which is Q's
//!
//! ```text
//! mkdir -p /tmp/nli-supported-unsupported-verified-unverifiable-degraded-stop
//! TMPDIR=/tmp/nli-supported-unsupported-verified-unverifiable-degraded-stop/ \
//!   cargo test --workspace --locked --no-fail-fast
//! ```
//!
//! Five failures, deterministic, against a baseline of 2,445/0 at `a8a14c2`
//! reproduced before any delta was trusted. Each guard asserts that engine
//! vocabulary never reaches a user-facing surface, and each searched the whole
//! rendered string — including the absolute path that surface legitimately
//! prints.
//!
//! **D25 closed one instance and Q proved it did not close the class.** That is
//! the entire reason this file exists: the earlier fix stripped one test's own
//! temp root, which is a fix for the test that noticed, not for the mistake.
//! Q also notes `--no-fail-fast` was the difference between finding one and
//! finding five.
//!
//! # What is asserted here, and what cannot be
//!
//! The workspace cannot be re-run under a hostile `TMPDIR` from inside itself.
//! So this file asserts the two things that are checkable and that together
//! close the class: the **stripper** does its job on Q's exact directory name,
//! and **no test in this directory calls the path-unaware census on rendered
//! output** — a source census, because the alternative is trusting that nobody
//! writes the fifth one again.

use std::collections::BTreeSet;

/// Q's directory name, verbatim. Every word in it is a census term.
const HOSTILE: &str = "/tmp/nli-supported-unsupported-verified-unverifiable-degraded-stop";

#[test]
fn q_s_own_directory_name_reaches_no_census() {
    let line = format!("hooks   {HOSTILE}/x/settings.json");
    assert_eq!(
        thruline_core::voice::banned_in_output(&line),
        None,
        "Q's TMPDIR still fails the census: {line}"
    );
    // And the raw-`contains` shape, which is the fifth guard's mechanism and a
    // different call: `a207_check_wrapper.rs` asserts on `contains("verified")`
    // rather than through `banned_in`.
    assert!(
        !thruline_core::voice::without_paths(&line)
            .to_lowercase()
            .contains("verified"),
        "the stripper leaves `verified` behind for a raw contains check"
    );

    // The control, and it is the whole argument: a word the PRODUCT wrote is
    // still caught. Without this, an implementation that strips everything
    // passes.
    let authored = format!("chain UNVERIFIED   {HOSTILE}/x");
    assert_eq!(
        thruline_core::voice::banned_in_output(&authored),
        Some("unverified"),
        "stripping paths also stopped the census seeing authored words"
    );
    // A RELATIVE path is authored and stays in the search space.
    assert!(
        thruline_core::voice::without_paths("see docs/STATUS.md").contains("docs/STATUS.md"),
        "a relative path was stripped; the product chooses those"
    );
}

/// The class guard: no CLI test may census rendered output with the
/// path-unaware call.
///
/// Derived from the source rather than from a list, the way this program's
/// other censuses are, because a list of five is a list that becomes six.
#[test]
fn no_cli_test_censuses_rendered_output_with_the_path_unaware_call() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    // Assembled, so this file does not match itself.
    let unaware = format!("{}{}", "banned_in", "(");
    let aware = format!("{}{}", "banned_in_output", "(");

    let mut offenders: BTreeSet<String> = BTreeSet::new();
    let mut scanned = 0usize;
    for entry in std::fs::read_dir(&dir).expect("tests dir").flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        scanned += 1;
        for (i, line) in text.lines().enumerate() {
            let bare = line.trim_start();
            if bare.starts_with("//") {
                continue;
            }
            if line.contains(&unaware) && !line.contains(&aware) {
                offenders.insert(format!(
                    "{}:{}: {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    i + 1,
                    bare.trim()
                ));
            }
        }
    }

    assert!(
        scanned > 20,
        "the census read only {scanned} files — it is searching the wrong place, \
         which is how a census passes forever"
    );
    assert!(
        offenders.is_empty(),
        "a CLI test censuses rendered output with the path-unaware call, so a \
         TMPDIR the operating system chose can fail it:\n  {}\n\nUse \
         `voice::banned_in_output`, which strips paths the product did not \
         author (Q's D24).",
        offenders.into_iter().collect::<Vec<_>>().join("\n  ")
    );
}

/// The sixth instance, which running Q's own command found and the report did
/// not list.
///
/// `a207_check_wrapper.rs` prints a scope as
/// `bash:/private/tmp/…/repo:./test.sh` — the path is **embedded in a token
/// that begins with `bash:`**, so a stripper that filtered tokens *starting*
/// with `/` walked straight past it. The first version of the class fix did
/// exactly that and this case is why it changed.
///
/// The lesson is the same one as the count: running the reported reproduction
/// beats reading the report. Q listed five; the environment produced six.
#[test]
fn a_path_embedded_after_a_colon_is_still_a_path() {
    let line = format!("scope bash:{HOSTILE}/.tmpc9cbvu/repo:./test.sh (test)");
    assert!(
        !thruline_core::voice::without_paths(&line)
            .to_lowercase()
            .contains("verified"),
        "an absolute path embedded after `bash:` survived the stripper: {}",
        thruline_core::voice::without_paths(&line)
    );
    assert_eq!(thruline_core::voice::banned_in_output(&line), None);

    // The control that this fix must not break, and it is the one that failed
    // first: a RELATIVE path is a word the product chose and stays whole.
    let rel = "see docs/STATUS.md and crates/core/src/voice.rs";
    assert_eq!(
        thruline_core::voice::without_paths(rel),
        rel,
        "a relative path was stripped; the product chooses those"
    );
}
