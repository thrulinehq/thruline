//! Label-order calibration. This is the most load-bearing test in the project.
//!
//! A wrong entailment index does not crash and does not fail loudly — it
//! silently inverts every verdict the product emits. Supported claims become ⚠
//! and manufactured ones become ✅, and the tool confidently reports the exact
//! opposite of the truth. So the mapping is proven twice: read from the
//! model's own `config.json`, then demonstrated on pairs whose answer is not
//! in dispute.
//!
//! Skips (does not fail) when the model is absent, so a fresh clone still runs
//! `cargo test`. CI that must enforce it should assert the model is installed.

//! Requires the `ort` feature: this file exercises the local ONNX
//! entailment backend, which `--no-default-features` removes. Compiled out
//! rather than skipped, so a feature-off build has nothing to link.
#![cfg(feature = "ort")]

use thruline_core::derive::EvidenceRow;
use thruline_core::nli::{EntailmentScorer, OrtNli, Scores};
use thruline_core::premise;

/// The configured `hi` threshold — the bar the product actually confirms at.
fn default_hi() -> f32 {
    thruline_core::config::Config::default().thresholds.hi
}

/// Build the evidence row a Bash cluster would produce, so the calibration
/// exercises the real premise renderer rather than a hand-written string.
fn bash(cmd: &str, tail: &str) -> EvidenceRow {
    EvidenceRow {
        kind: "command",
        path: None,
        summary: String::new(),
        tool_name: "Bash".to_string(),
        command: Some(cmd.to_string()),
        result_tail: Some(tail.to_string()),
        new_content: None,
        result_excerpt: None,
        tool_use_id: None,
        is_error: false,
        tool_input: None,
        raw_file: None,
        raw_line: None,
        session_id: None,
        seq: None,
        cwd: None,
        git_branch: None,
        command_identity: None,
        raw_sha256: None,
    }
}

fn scorer() -> Option<OrtNli> {
    let required = std::env::var_os("THRULINE_REQUIRE_NLI_MODEL").is_some();
    let dir = match OrtNli::default_dir() {
        Ok(dir) => dir,
        Err(error) if required => {
            panic!("release calibration could not resolve the production model directory: {error}")
        }
        Err(error) => {
            eprintln!("SKIP: could not resolve the production model directory: {error}");
            return None;
        }
    };
    if !dir.join("model_quantized.onnx").exists() {
        let message = format!(
            "no verified A18 model at {} — supply the three local files with `thruline model import --from DIR --apply` and confirm them with `thruline model status`",
            dir.display()
        );
        if required {
            panic!("release calibration requires the provisioned model: {message}");
        }
        eprintln!("SKIP: {message}");
        return None;
    }
    Some(OrtNli::open(dir).expect("model dir is present but failed to load"))
}

fn show(tag: &str, s: Scores) {
    eprintln!(
        "  {tag:<38} ent={:.3} neu={:.3} con={:.3}",
        s.entailment, s.neutral, s.contradiction
    );
}

#[test]
fn label_indices_come_from_the_model_config() {
    let Some(nli) = scorer() else { return };
    let lm = nli.labels();
    // Pinned from the downloaded config (A18). If a future model ships a
    // different order this fails, which is the point — it must be noticed.
    assert_eq!(lm.contradiction, 0, "contradiction index moved");
    assert_eq!(lm.entailment, 1, "entailment index moved");
    assert_eq!(lm.neutral, 2, "neutral index moved");
}

#[test]
fn calibration_entailment_direction_and_index() {
    let Some(mut nli) = scorer() else { return };

    // Premise entails hypothesis. Not ambiguous to any competent NLI model.
    let fwd = nli
        .score("A cat was sitting on the mat.", "The cat sat.")
        .expect("score");
    show("entails (premise=evidence)", fwd);
    assert!(
        fwd.dominant_is_entailment(),
        "the specific pair the brief names must read as entailment; \
         if this fails the label index is wrong and every verdict is inverted: {fwd:?}"
    );

    // ORIENTATION. Reversed, the general statement "The cat sat." does not
    // establish the specific "A cat was sitting on the mat.", so entailment
    // must not dominate. This is what pins premise=evidence / hypothesis=claim.
    let rev = nli
        .score("The cat sat.", "A cat was sitting on the mat.")
        .expect("score");
    show("reversed (must NOT entail)", rev);
    assert!(
        !rev.dominant_is_entailment(),
        "reversing premise and hypothesis still read as entailment — the \
         orientation is not doing any work: {rev:?}"
    );
    assert!(
        fwd.entailment > rev.entailment,
        "the correct direction must score higher than the reverse: \
         fwd={:.3} rev={:.3}",
        fwd.entailment,
        rev.entailment
    );

    // A flat contradiction must land on the contradiction head.
    let con = nli
        .score(
            "The test suite passed with 0 failures.",
            "All the tests failed.",
        )
        .expect("score");
    show("contradiction", con);
    assert!(
        con.dominant_is_contradiction(),
        "known contradiction did not read as contradiction — label order is \
         wrong: {con:?}"
    );
}

/// The product's actual question, in miniature: does a tool result entail what
/// the assistant said about it?
#[test]
fn calibration_on_the_shape_the_product_actually_asks() {
    let Some(mut nli) = scorer() else { return };

    // Through the production renderer, not a hand-written premise.
    let good = premise::render(&bash("cargo test", "test result: ok. 214 passed; 0 failed"));
    let bad = premise::render(&bash(
        "cargo test",
        "test result: FAILED. 31 passed; 2 failed",
    ));
    eprintln!("  premise(ok)      {good}");
    eprintln!("  premise(failed)  {bad}");

    let truthful = nli.score(&good, "All 214 tests pass.").expect("score");
    show("truthful marker claim", truthful);
    let misstated = nli.score(&bad, "All 33 tests pass.").expect("score");
    show("misstated marker claim", misstated);

    // A truthful marker claim must be confidently supported, not merely ahead
    // of the lie. Scraping past on a 0.008-vs-0.000 margin is how a false ⚠
    // ships.
    assert!(
        truthful.dominant_is_entailment() && truthful.entailment > 0.5,
        "a truthful marker claim must read as entailment through the real \
         renderer, or every honest test report becomes a false alarm: {truthful:?}"
    );
    // The misstated one is the single most valuable catch the product makes.
    assert!(
        misstated.entailment < 0.1,
        "a claim contradicted by its own tool output must not look supported: {misstated:?}"
    );
}

/// M3.4's fallback finding, re-measured after M3.8b raised parsed coverage
/// from 34.5% to 100% on the real corpus.
///
/// This is the check that M3.8b earned its coverage rather than faking it.
/// The old shape — "The command X was run. Its output ended with: <blob>" —
/// asserted nothing, and M3.4 measured the consequence: a TRUE paraphrase
/// scored 0.018 while an outright FALSEHOOD scored 0.125, so the ranking
/// inverted and a low score carried no information.
///
/// The new shape leads with a fact the harness gave us ("completed
/// successfully") and appends the blob only as detail. If the inversion
/// survived that change, then tagging the shape `parsed` would be defeating
/// the fidelity gate instead of passing it, and the branch must be reverted.
#[test]
fn raising_parsed_coverage_did_not_just_relabel_the_problem() {
    let Some(mut nli) = scorer() else { return };

    let blob = "smoke: checkout ok | webhook ok | 3 endpoints warm";
    let now = premise::render_tagged(&bash("./scripts/smoke.sh", blob));
    eprintln!("  premise(now)   {}", now.text);
    assert_eq!(
        now.fidelity.as_str(),
        "parsed",
        "M3.8b tags this shape parsed; if that changed, re-derive the finding below"
    );

    // The exact wording M3.4 measured, reconstructed by hand so the comparison
    // is like-for-like rather than a memory of an older build.
    let old = format!("The command ./scripts/smoke.sh was run. Its output ended with: {blob}");

    let probes: [(&str, &str); 3] = [
        (
            "true, near-verbatim",
            "The smoke script reported checkout and webhook OK.",
        ),
        (
            "true, paraphrased",
            "Checkout and the webhook are both healthy.",
        ),
        ("FALSE", "The smoke script reported three failures."),
    ];
    let mut old_scores = [0f32; 3];
    let mut new_scores = [0f32; 3];
    for (i, (tag, claim)) in probes.iter().enumerate() {
        old_scores[i] = nli.score(&old, claim).expect("score").entailment;
        new_scores[i] = nli.score(&now.text, claim).expect("score").entailment;
        eprintln!(
            "  {tag:<22} old={:.3}  new={:.3}",
            old_scores[i], new_scores[i]
        );
    }

    // The property that matters is not that every true claim scores high —
    // a heavily paraphrased one still may not, and abstention handles that.
    // It is that a FALSEHOOD must not outrank a TRUTH, because that is what
    // makes a low score uninformative rather than merely weak.
    let old_inverted = old_scores[1] < old_scores[2];
    let new_inverted = new_scores[1] < new_scores[2];
    eprintln!("  ranking inverted?   old={old_inverted}  new={new_inverted}");

    assert!(
        !new_inverted,
        "the new shape still ranks a FALSE claim ({:.3}) above a TRUE paraphrase \
         ({:.3}). Tagging it `parsed` would be defeating the fidelity gate rather \
         than earning past it — revert the exit-status branch in premise.rs.",
        new_scores[2], new_scores[1]
    );
    // And the shape must still CONFIRM a claim it genuinely supports, judged
    // against the threshold the product actually uses rather than a round
    // number. Measured 2026-07-29: near-verbatim 0.915 -> 0.811. The grounded
    // lead costs a little surface overlap, which is a real and acceptable
    // price — 0.811 still clears `hi` = 0.60 comfortably, and it is paid to
    // move the FALSE claim from 0.125 to 0.001, which is what removes the
    // inversion and makes a low score mean something again.
    let hi = default_hi();
    assert!(
        new_scores[0] >= hi,
        "a true claim the premise plainly supports scored {:.3}, below hi = {hi:.2}",
        new_scores[0]
    );
}

/// The `fallback` tag is not dead machinery just because this corpus no longer
/// triggers it: an edit whose target we could not recover from either the input
/// or the result is still evidence we cannot interpret, and must still abstain.
#[test]
fn fallback_still_exists_for_evidence_we_genuinely_cannot_read() {
    let mut r = bash("x", "y");
    r.kind = "edit";
    r.tool_name = "Edit".to_string();
    r.command = None;
    r.result_tail = None;
    r.path = None;
    let rendered = premise::render_tagged(&r);
    assert_eq!(
        rendered.fidelity.as_str(),
        "fallback",
        "an edit with no recoverable path must stay fallback: {rendered:?}"
    );
}

#[test]
fn max_pooling_picks_the_supporting_candidate_not_the_first() {
    let Some(mut nli) = scorer() else { return };
    let premises = vec![
        "Read file src/unrelated.rs.".to_string(),
        "Ran command `npm test`. Result: 12 passing.".to_string(),
        "Listed directory docs/.".to_string(),
    ];
    let (idx, best) = nli
        .best_of(&premises, "The tests pass.")
        .expect("score")
        .expect("some candidate");
    show("best of three", best);
    assert_eq!(
        idx, 1,
        "max-pooling must attribute the score to the candidate that actually \
         supports the claim, not merely return a number"
    );
}
