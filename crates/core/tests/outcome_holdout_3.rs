//! §5e/§5h holdout-3, sealed at M3.12.1 and opened exactly once at M3.12.3.
//!
//! Sealed hash `64e7bb374b5c77d0f87851c8b026efd3d0a519ab5e5aa02d33700c42ca8d8bd2`,
//! recorded in A49 before any M3.12.2 classifier or renderer change existed.
//!
//! Unlike holdout-1 and holdout-2, every case here declares the **span it lives
//! in**, and the harness classifies through the provenance-aware entry point.
//! That is what makes it able to test §5g at all: a bare string cannot express
//! provenance, which is precisely why holdout-2's command-source trap could only
//! ever exercise the second line of defence.
//!
//! Real and trap counters are separate (A47).

use std::collections::BTreeMap;

use thruline_core::premise::{classify_span, Outcome, SpanKind};

#[derive(serde::Deserialize)]
struct Case {
    id: String,
    grammar: String,
    provenance: String,
    family: String,
    passed: Option<u32>,
    failed: Option<u32>,
    #[serde(default)]
    must_abstain: bool,
    text: String,
}

#[derive(serde::Deserialize)]
struct Holdout {
    cases: Vec<Case>,
}

fn observed(o: &Outcome) -> (&'static str, Option<u32>, Option<u32>) {
    match o {
        Outcome::Tests { passed, failed } => ("test", *passed, *failed),
        Outcome::Succeeded | Outcome::Failed => ("build", None, None),
        Outcome::Unknown => ("none", None, None),
    }
}

#[test]
#[ignore = "BURNED after its one M3.12.3 opening; result recorded in A49"]
fn sealed_holdout_3() {
    // A43 burn protection, mechanical — `#[ignore]` alone does not stop
    // `cargo test -- --ignored`, as M3.12.2 demonstrated the hard way.
    if std::env::var("THRULINE_RUN_BURNED_HOLDOUT").as_deref() != Ok("yes") {
        println!("SKIPPED: burned partition. Set THRULINE_RUN_BURNED_HOLDOUT=yes to force.");
        return;
    }
    let raw = include_str!("fixtures/outcome-holdout-3.json");
    let h: Holdout = serde_json::from_str(raw).expect("holdout-3 parses");

    let (mut real, mut real_correct, mut abstained) = (0usize, 0usize, 0usize);
    let (mut traps, mut trap_correct) = (0usize, 0usize);
    let mut wrong: Vec<String> = Vec::new();
    // Per-provenance trap tallies: §5g's claim is that provenance decides, so
    // the report has to show which line of defence held.
    let mut trap_by_prov: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut real_by_grammar: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for c in &h.cases {
        let kind = SpanKind::parse(&c.provenance)
            .unwrap_or_else(|| panic!("{}: unknown provenance {:?}", c.id, c.provenance));
        let (family, passed, failed) = observed(&classify_span(kind, &c.text));

        if c.must_abstain {
            traps += 1;
            let e = trap_by_prov.entry(c.provenance.clone()).or_insert((0, 0));
            e.1 += 1;
            if family == "none" {
                trap_correct += 1;
                e.0 += 1;
            } else {
                wrong.push(format!(
                    "  TRAP PARSED   {:<5} [{}] {:<52} got {family} {passed:?}/{failed:?}",
                    c.id, c.provenance, c.grammar
                ));
            }
            continue;
        }

        real += 1;
        let e = real_by_grammar.entry(c.grammar.clone()).or_insert((0, 0));
        e.1 += 1;
        if family == "none" {
            abstained += 1;
        } else if family == c.family && passed == c.passed && failed == c.failed {
            real_correct += 1;
            e.0 += 1;
        } else {
            wrong.push(format!(
                "  WRONG         {:<5} [{}] {:<52} want {} {:?}/{:?}, got {family} {passed:?}/{failed:?}",
                c.id, c.provenance, c.grammar, c.family, c.passed, c.failed
            ));
        }
    }

    let coverage = 100.0 * real_correct as f64 / real.max(1) as f64;
    println!("\n  SEALED HOLDOUT-3 — opened once at M3.12.3");
    println!("  real formats     {real_correct}/{real} = {coverage:.1}%   (gate 70%)");
    println!("  abstentions      {abstained}");
    println!("  traps abstained  {trap_correct}/{traps}");
    println!("  MISCLASSIFIED    {}   (gate ZERO)", wrong.len());
    println!("\n  traps by provenance:");
    for (prov, (ok, n)) in &trap_by_prov {
        println!("    {ok}/{n:<3} {prov}");
    }
    println!("\n  real formats by grammar:");
    for (g, (ok, n)) in &real_by_grammar {
        println!("    {ok}/{n:<3} {g}");
    }
    if !wrong.is_empty() {
        println!("\n  failures:");
        for w in &wrong {
            println!("{w}");
        }
    }

    assert!(
        wrong.is_empty(),
        "§5e: misclassification is worse than abstention, and the gate is ZERO.\n{}",
        wrong.join("\n")
    );
    assert!(
        coverage >= 70.0,
        "holdout-3 real-format coverage {coverage:.1}% < 70%"
    );
}
