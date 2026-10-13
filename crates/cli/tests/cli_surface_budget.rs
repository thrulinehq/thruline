//! The CLI's subcommand budget, because overflowing it aborts an unrelated test.
//!
//! # The defect this exists against, measured twice
//!
//! clap's derive builds every **inline** `Command` variant's arguments inside
//! one generated function, and that function's stack frame is what runs out of
//! a test thread's stack. Both times it happened in this project the symptom
//! was the same and useless: `cargo test -p thruline-cli --bin thruline`
//! aborting with `has overflowed its stack` inside a test that had nothing to
//! do with the command just added — first `entity_cli_tests`, then
//! `model_cli_tests::status_accepts_an_explicit_directory_without_other_inputs`.
//! Neither message named the cause, and nothing pointed at the enum.
//!
//! The fix both times was to move a variant's fields into a `clap::Args` struct,
//! which gives them their own `augment_args` function and their own frame. This
//! test makes that a rule with a number instead of folklore.
//!
//! # It is a proxy, and it is labelled as one
//!
//! The real quantity is the generated function's stack usage, which cannot be
//! measured from here — a stack overflow aborts the process rather than failing
//! a test, so it cannot even be caught. The variant count is a proxy for it.
//! What the proxy buys is a **clean failure that names the cause**: a new inline
//! variant fails here, saying to flatten it, instead of aborting a test three
//! modules away.

use std::path::{Path, PathBuf};

/// Measured, not chosen: 51 inline variants overflowed a test thread's stack.
/// Flattening one brought it back. The budget is therefore the number that was
/// observed to work, and it does not grow without someone changing this line
/// and saying why.
const INLINE_BUDGET: usize = 50;

fn main_rs() -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// `(inline, tuple, unit)` variant counts in `enum Command`.
fn variant_shapes(src: &str) -> (usize, usize, usize) {
    let start = src.find("enum Command {").expect("enum Command");
    let body = &src[start..src[start..].find("\n}\n").expect("end of enum") + start];
    let (mut inline, mut tuple, mut unit) = (0, 0, 0);
    let mut depth = 0usize;
    for line in body.lines().skip(1) {
        let st = line.trim();
        if depth > 0 {
            if st == "}," {
                depth = 0;
            }
            continue;
        }
        let is_name = |s: &str| {
            s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && s.chars().all(|c| c.is_ascii_alphanumeric())
        };
        if let Some(name) = st.strip_suffix('{').map(str::trim) {
            if is_name(name) {
                inline += 1;
                depth = 1;
            }
        } else if let Some(rest) = st.strip_suffix("),") {
            if let Some((name, _)) = rest.split_once('(') {
                if is_name(name) {
                    tuple += 1;
                }
            }
        } else if let Some(name) = st.strip_suffix(',') {
            if is_name(name) {
                unit += 1;
            }
        }
    }
    (inline, tuple, unit)
}

#[test]
fn the_command_enum_stays_within_its_measured_inline_budget() {
    let (inline, tuple, unit) = variant_shapes(&main_rs());
    // Positive control: the walk found a real enum, not an empty one.
    assert!(
        inline + tuple + unit > 40,
        "the variant walk found only {} variants, so the budget below is vacuous",
        inline + tuple + unit
    );
    assert!(
        tuple >= 5,
        "the walk cannot see flattened variants ({tuple} found), so it would not \
         notice one being un-flattened"
    );
    assert!(
        inline <= INLINE_BUDGET,
        "`enum Command` has {inline} inline variants and the measured budget is \
         {INLINE_BUDGET}. clap's derive builds them all in one function, and that \
         function's stack frame overflows a test thread — the symptom is \
         `cargo test -p thruline-cli --bin thruline` aborting inside a test that has \
         nothing to do with your change. Give the new command a `#[derive(clap::Args)]` \
         struct and make the variant a tuple, as `PolicyArgs` and `ImportLegacyArgs` \
         do; that gives its arguments their own frame."
    );
}

/// The parser really does build, on the same kind of thread the failure appears
/// on. This is the check the budget is a proxy for, run in a subprocess so a
/// stack overflow is a reported exit status rather than an abort that takes the
/// whole test binary with it.
#[test]
fn the_parser_builds_on_a_test_sized_stack() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--help")
        .env("RUST_MIN_STACK", (2 * 1024 * 1024).to_string())
        .output()
        .expect("run --help");
    assert!(
        out.status.success(),
        "`thruline --help` exited {:?}: the parser could not be built. If this is a \
         stack overflow, the command enum has outgrown its budget.",
        out.status
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Commands:"), "{text}");
    // Every command added this milestone is reachable.
    for verb in ["policy", "sync", "demo", "bench"] {
        assert!(text.contains(verb), "`{verb}` is missing from --help");
    }
}

/// The counter itself, against a source it can be checked by hand.
///
/// Mutating the real `main.rs` to validate this guard does not compile — the
/// dispatch arms have to change with the variant — so the control is here, over
/// a synthetic enum whose shape is obvious. Without it, `variant_shapes` could
/// be counting nothing and the budget would pass forever.
#[test]
fn the_variant_walk_counts_the_shapes_it_claims_to() {
    let src = "\
#[derive(Subcommand)]
enum Command {
    /// doc comment
    Inline {
        #[arg(long)]
        a: bool,
        b: Option<String>,
    },
    Another {
        c: u8,
    },
    Flattened(SomeArgs),
    AlsoFlattened(OtherArgs),
    Bare,
}
";
    assert_eq!(variant_shapes(src), (2, 2, 1));

    // And un-flattening one moves the count from `tuple` to `inline`, which is
    // exactly the edit the budget is watching for.
    let unflattened = src.replace(
        "    Flattened(SomeArgs),",
        "    Flattened {\n        x: bool,\n    },",
    );
    assert_eq!(variant_shapes(&unflattened), (3, 1, 1));
}
