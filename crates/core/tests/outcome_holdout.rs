//! §5e — the sealed holdout, opened once at M3.10.4.
//!
//! Grammars mined from market + thruline and committed at
//! `cfb1c032c025bbdf0be309aa2abd3d411dd99fd40f9bcc39b7efaabcae059984` **before**
//! any parser work in M3.10.2, and never executed during it. The gap between
//! this number and the dev number is the only honest generalization measurement
//! available, and it exists because A36 found the eval corpus and the parser had
//! been written against each other.
//!
//! Runs in the default suite: pure string classification, no model, no ledger.

use std::collections::BTreeMap;

use thruline_core::premise::{classify_outcome, Outcome};

#[derive(serde::Deserialize)]
struct Case {
    id: String,
    family: String,
    passed: Option<u32>,
    failed: Option<u32>,
    #[allow(dead_code)]
    ok: Option<bool>,
    #[serde(default)]
    must_abstain: bool,
    grammar: String,
    text: String,
}

#[derive(serde::Deserialize)]
struct Holdout {
    cases: Vec<Case>,
}

/// What the classifier said, in the holdout's own vocabulary.
fn observed(o: &Outcome) -> (&'static str, Option<u32>, Option<u32>) {
    match o {
        Outcome::Tests { passed, failed } => ("test", *passed, *failed),
        Outcome::Succeeded => ("build", None, None),
        Outcome::Failed => ("build", None, None),
        Outcome::Unknown => ("none", None, None),
    }
}

#[test]
#[ignore = "BURNED after its one M3.10.4 opening; result recorded in A43"]
fn sealed_holdout() {
    // A43 burn protection, mechanical. `#[ignore]` is NOT enough: `cargo test
    // -- --ignored` runs ignored tests, and that is exactly how this partition
    // was re-executed during M3.12.2's invariant sweep. A burned partition can
    // add no generalization evidence, so the body refuses unless a human names
    // it explicitly.
    if std::env::var("THRULINE_RUN_BURNED_HOLDOUT").as_deref() != Ok("yes") {
        println!("SKIPPED: burned partition. Set THRULINE_RUN_BURNED_HOLDOUT=yes to force.");
        return;
    }

    let raw = include_str!("fixtures/outcome-holdout.json");
    let h: Holdout = serde_json::from_str(raw).expect("holdout parses");

    let (mut correct, mut abstained, mut misclassified) = (0usize, 0usize, 0usize);
    let mut real = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut by_grammar: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for c in &h.cases {
        let got = classify_outcome(&c.text);
        let (fam, p, f) = observed(&got);

        if c.must_abstain {
            // A trap that parses is a misclassification — the worst outcome
            // there is, because it manufactures a confident false receipt.
            if fam == "none" {
                correct += 1;
            } else {
                misclassified += 1;
                failures.push(format!(
                    "  TRAP PARSED  {:<4} {:<44} got {fam} p={p:?} f={f:?}",
                    c.id, c.grammar
                ));
            }
            continue;
        }

        real += 1;
        let e = by_grammar.entry(c.grammar.clone()).or_insert((0, 0));
        e.1 += 1;

        if fam == "none" {
            // Abstention costs coverage, never correctness (§5e).
            abstained += 1;
            continue;
        }
        if fam != c.family {
            misclassified += 1;
            failures.push(format!(
                "  WRONG FAMILY {:<4} {:<44} want {} got {fam}",
                c.id, c.grammar, c.family
            ));
            continue;
        }
        // `null` in the expectation means "the format states no count"; a
        // parser that invents one there is misclassifying.
        let wrong_count = (c.passed.is_some() && p.is_some() && c.passed != p)
            || (c.failed.is_some() && f.is_some() && c.failed != f)
            || (c.passed.is_none() && p.is_some() && c.family == "test")
            || (c.failed.is_none() && f.is_some() && c.family == "test");
        if wrong_count {
            misclassified += 1;
            failures.push(format!(
                "  WRONG COUNT  {:<4} {:<44} want p={:?} f={:?} got p={p:?} f={f:?}",
                c.id, c.grammar, c.passed, c.failed
            ));
            continue;
        }
        correct += 1;
        e.0 += 1;
    }

    let traps = h.cases.len() - real;
    // A47: `correct` here counts traps too, so this formula is only right when
    // every trap abstained — which it did at M3.10.4 (0 misclassifications),
    // making the reported 95.8% correct by luck rather than by construction.
    // Holdout-1 is burned and cannot be re-run, so the number stands as
    // recorded; the formula is corrected in place so it cannot mislead a
    // future reader of this file.
    let real_correct = correct.saturating_sub(traps);
    let coverage = 100.0 * real_correct as f64 / real.max(1) as f64;
    println!("\n  SEALED HOLDOUT (market + thruline grammars, never seen during M3.10.2)");
    println!("  real-format cases   {real}");
    println!("  traps               {traps}");
    println!("  correct             {correct}  (includes traps correctly abstained)");
    println!("  abstained           {abstained}");
    println!("  MISCLASSIFIED       {misclassified}");
    println!("  COVERAGE            {coverage:.1}%  (gate 70%)\n");
    println!("  per grammar:");
    for (g, (ok, n)) in &by_grammar {
        println!("    {ok}/{n:<3} {g}");
    }
    if !failures.is_empty() {
        println!("\n  failures:");
        for f in &failures {
            println!("{f}");
        }
    }

    assert_eq!(
        misclassified,
        0,
        "§5e: misclassification is worse than abstention, and the gate is ZERO.\n{}",
        failures.join("\n")
    );
    assert!(
        coverage >= 70.0,
        "holdout coverage {coverage:.1}% is below the 70% gate"
    );
}
