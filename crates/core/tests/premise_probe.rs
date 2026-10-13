//! Renderer-wording probe (M3.6 calibration aid).
//!
//! `#[ignore]`d. Its job is to make renderer changes measurements rather than
//! guesses, per CLAUDE.md's `premise::render` invariant.
//!
//! ```text
//! cargo test --release --test premise_probe -- --ignored --nocapture
//! ```

//! Requires the `ort` feature: this file exercises the local ONNX
//! entailment backend, which `--no-default-features` removes. Compiled out
//! rather than skipped, so a feature-off build has nothing to link.
#![cfg(feature = "ort")]

use thruline_core::nli::{EntailmentScorer, OrtNli};

fn scorer() -> Option<OrtNli> {
    let dir = OrtNli::default_dir().ok()?;
    dir.join("model_quantized.onnx")
        .exists()
        .then(|| OrtNli::open(&dir).expect("load"))
}

#[test]
#[ignore]
fn probe() {
    let Some(mut nli) = scorer() else { return };

    let block = |nli: &mut OrtNli, title: &str, premises: &[&str], claims: &[(&str, &str)]| {
        eprintln!("\n=== {title} ===");
        for p in premises {
            eprintln!("  P: {p}");
            for (tag, c) in claims {
                let s = nli.score(p, c).expect("score");
                eprintln!(
                    "     {tag:<28} ent={:.3} neu={:.3} con={:.3}  {}  H: {c}",
                    s.entailment,
                    s.neutral,
                    s.contradiction,
                    if s.dominant_is_contradiction() {
                        "CONTRA"
                    } else {
                        "      "
                    },
                );
            }
        }
    };

    // 0. THE SEPARATOR QUESTION. A low entailment score means two very
    //    different things: "this claim conflicts with the evidence" (accuse)
    //    and "I cannot tell" (abstain). The contradiction head distinguishes
    //    them, and we currently discard it.
    block(
        &mut nli,
        "does the contradiction head separate FALSE from MERELY-UNCONFIRMED?",
        &["The command cargo test --workspace completed successfully. All 96 tests passed."],
        &[
            (
                "TRUE hard paraphrase",
                "The suite is green — 96 passing, nothing broken.",
            ),
            ("TRUE plain", "All 96 tests pass."),
            ("FALSE, contradicts", "The test suite failed."),
        ],
    );
    block(
        &mut nli,
        "the planted fakes — must stay contradiction-dominant",
        &[
            "The command cargo test --all was run and it failed. 31 tests passed and 2 tests failed.",
            "The tool Read was run /repo/src/lib.rs. It returned: pub mod chain;",
            "The file /repo/README.md was edited. The new content includes: # Project",
        ],
        &[
            ("misstated 33/33", "33/33 tests pass."),
            ("fabricated tests", "All 214 tests pass."),
            ("fabricated edit", "I've added retry logic to `src/webhook/client.rs`."),
        ],
    );

    // 1. A FAILING run vs an "all pass" claim. Currently 0.633 — a false OK on
    //    the crown-jewel case. Which wording tanks it?
    block(
        &mut nli,
        "failing run vs all-pass claim (want LOW)",
        &[
            "The command cargo test --all was run. 31 tests passed and 2 tests failed.",
            "The command cargo test --all was run and it failed. 2 tests failed.",
            "The command cargo test --all failed. Some tests did not pass.",
            "The test run failed. Not all tests passed.",
            "The command cargo test --all was run and it failed: 2 of 33 tests failed.",
        ],
        &[
            ("misstated 33/33", "33/33 tests pass."),
            ("misstated 48/48", "48/48 pytest tests pass."),
            ("truthful failure", "2 tests are failing."),
        ],
    );

    // 2. A PASSING run vs a paraphrased true claim. Currently 0.005 — a false
    //    warning on honest work, which is the failure that matters most.
    block(
        &mut nli,
        "passing run vs paraphrased true claim (want HIGH)",
        &[
            "The command cargo test --workspace completed successfully. All 96 tests passed.",
            "The test suite passed. All 96 tests passed.",
            "The tests passed successfully.",
            "The command cargo test --workspace completed successfully. The test suite is green.",
        ],
        &[
            (
                "paraphrase",
                "The suite is green — 96 passing, nothing broken.",
            ),
            ("plain", "All 96 tests pass."),
        ],
    );

    // 2b. Can the claim's positive core be scored when a negation clause is
    //     what tanked it? If so the fix confirms rather than abstains.
    block(
        &mut nli,
        "negation-stripped claim variant (want HIGH)",
        &["The command cargo test --workspace completed successfully. All 96 tests passed."],
        &[
            (
                "full claim",
                "The suite is green — 96 passing, nothing broken.",
            ),
            ("core only", "The suite is green — 96 passing."),
            ("core only, plainer", "The suite is green."),
        ],
    );

    // 2c. Failing-run wording that KEEPS the pass count.
    block(
        &mut nli,
        "failing run, keeping both counts (want LOW misstated, HIGH truthful)",
        &[
            "The command cargo test --all was run and it failed. 31 tests passed and 2 tests failed.",
            "The command cargo test --all was run and it failed. 2 tests failed.",
        ],
        &[
            ("misstated 33/33", "33/33 tests pass."),
            ("truthful failure", "2 tests are failing."),
            ("truthful partial", "31 tests passed."),
        ],
    );

    // 3. A proposal as hypothesis: future-tense intent can never be entailed by
    //    past-tense evidence, however well the action matches.
    block(
        &mut nli,
        "proposal hypothesis, raw vs normalised (want HIGH for the match)",
        &[
            "The file /repo/src/api/quotasluice.rs was edited. The new content includes: \
             pub struct QuotaSluice { per_minute: u32 }",
        ],
        &[
            (
                "raw intent",
                "I'll add rate limiting to `src/api/quotasluice.rs`.",
            ),
            (
                "normalised",
                "Added rate limiting to `src/api/quotasluice.rs`.",
            ),
            (
                "normalised, no ticks",
                "Rate limiting was added to src/api/quotasluice.rs.",
            ),
        ],
    );
}
