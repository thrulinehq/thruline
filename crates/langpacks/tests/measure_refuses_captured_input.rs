//! `measure` refuses a `--corpus` that could hold captured data.
//!
//! # What this replaces, and why a dependency check was not enough
//!
//! `measure` prints text it has cut short: `head_token` cuts an assertion's
//! leading identifier to 48 characters, `one_line` cuts residue text to 140.
//! The census entry for those sites claims their input cannot carry a
//! credential, and that claim rested on *what the binary reads*.
//!
//! The guard first proposed for it checked the crate's **dependency graph** —
//! `thruline-langpacks` links no ledger, capture or spool crate, which is true
//! directly and transitively. It is also adjacent to the claim rather than
//! equal to it: the binary takes a `--corpus` path and reads through `std::fs`,
//! so it could be pointed at a spool with **no dependency change, no code
//! change and no new crate**. Forty-eight characters is short enough to cut a
//! credential below its detectable length, which is what the census exists to
//! catch.
//!
//! So the condition is checked where it lives — at the input — and this file is
//! what makes that mechanical rather than stated. When it passes, the census
//! entry's `UNGUARDED` mark comes off; that removal is the signal the work
//! happened.
//!
//! # The controls are the point
//!
//! A guard that refuses everything would pass every refusal case here and be
//! useless. Two controls run alongside: an ordinary corpus path must be
//! **accepted**, and a `..` that walks into a forbidden root must still be
//! **refused**.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_measure");

/// A temp directory that removes itself, under a name no other run can collide
/// with.
///
/// The first version of these tests used `std::env::temp_dir().join(
/// "langpacks-guard-refuse")` and never removed it. Two consequences, both
/// real and both found by reading rather than by a failure:
///
/// - **it leaked.** Both directories were still on disk after the night's runs,
///   which is one of the temp-leak causes DECISIONS #177's census went looking
///   for. A test that leaves state behind is a test that passes on a machine it
///   dirtied;
/// - **the name was fixed**, so two runs at once — `cargo test` runs integration
///   suites in parallel — would share a directory and could see each other's
///   `.thruline` roots. The refusal cases would still pass; the *control* could
///   fail or, worse, pass for the wrong reason.
///
/// The pid makes it unique per process and `Drop` removes it on every exit,
/// including a panicking assertion. Hand-written rather than `tempfile`,
/// because this crate's `Cargo.toml` is watched by the core's linkage census
/// and a dependency added to save ten lines is paid for in a census diff.
struct TempTree(std::path::PathBuf);

impl TempTree {
    fn new(tag: &str) -> TempTree {
        let p = std::env::temp_dir().join(format!("langpacks-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("create temp tree");
        TempTree(p)
    }
    fn join(&self, rel: &str) -> std::path::PathBuf {
        self.0.join(rel)
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One refusal route: a label, the environment it needs, and the path to try.
struct Case {
    label: &'static str,
    env: Vec<(&'static str, String)>,
    path: std::path::PathBuf,
}

fn run(env: &[(&str, &str)], corpus: &Path) -> (i32, String) {
    let mut c = Command::new(BIN);
    c.env_remove("THRULINE_HOME")
        .env_remove("THRULINE_HOOK_SPOOL");
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.arg("--corpus").arg(corpus).output().expect("run measure");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

/// Exit 2 and a refusal on stderr, for every route to captured data.
#[test]
fn a_corpus_under_a_captured_data_root_is_refused() {
    let tree = TempTree::new("guard-refuse");
    let tmp = tree.0.clone();
    let home = tree.join(".thruline");
    std::fs::create_dir_all(home.join("gate")).expect("mkdir");
    std::fs::create_dir_all(tmp.join("elsewhere/spool")).expect("mkdir");
    let hook = tmp.join("hook.jsonl");
    std::fs::write(&hook, b"{}\n").expect("write");

    let home_s = home.display().to_string();
    let hook_s = hook.display().to_string();
    let cases = vec![
        Case {
            label: "inside $THRULINE_HOME",
            env: vec![("THRULINE_HOME", home_s.clone())],
            path: home.join("gate"),
        },
        Case {
            label: "$THRULINE_HOME itself",
            env: vec![("THRULINE_HOME", home_s.clone())],
            path: home.clone(),
        },
        Case {
            label: "a `spool` path component, no env set",
            env: vec![],
            path: tmp.join("elsewhere/spool"),
        },
        Case {
            label: "$THRULINE_HOOK_SPOOL naming a file",
            env: vec![("THRULINE_HOOK_SPOOL", hook_s.clone())],
            path: hook.clone(),
        },
        Case {
            label: "`..` walking into a forbidden root",
            env: vec![("THRULINE_HOME", home_s.clone())],
            path: tmp.join("elsewhere/../.thruline/gate"),
        },
    ];

    for Case { label, env, path } in cases {
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (code, text) = run(&env, &path);
        // **3, not 2.** `measure` already exits 2 for an incomplete corpus, and
        // the two refusals must be distinguishable — see the note at the call
        // site. This assertion is what pins that apart.
        assert_eq!(
            code, 3,
            "{label}: expected exit 3, got {code}. Output:\n{text}"
        );
        assert!(
            text.contains("refusing --corpus"),
            "{label}: refused without saying so:\n{text}"
        );
    }
}

/// **The control.** An ordinary path must be ACCEPTED.
///
/// Without this, a guard that refused everything would pass the test above and
/// be worse than no guard — it would make the binary unusable while reading as
/// protection.
#[test]
fn an_ordinary_corpus_path_is_not_refused() {
    let tree = TempTree::new("guard-allow");
    let tmp = tree.join("corpus");
    std::fs::create_dir_all(&tmp).expect("mkdir");
    let home = tree.join(".thruline");
    std::fs::create_dir_all(&home).expect("mkdir");

    let (code, text) = run(&[("THRULINE_HOME", &home.display().to_string())], &tmp);
    // This binary exits 2 for an incomplete corpus, which this fixture is — so
    // the control asserts the GUARD's code (3) is absent, not that the run
    // succeeded. Asserting `!= 2` conflated the two refusals and is what
    // surfaced the shared exit code in the first place.
    assert_ne!(
        code, 3,
        "an ordinary corpus path was refused BY THE GUARD — it over-refuses:\n{text}"
    );
    assert!(
        !text.contains("refusing --corpus"),
        "an ordinary corpus path drew a refusal:\n{text}"
    );
}
