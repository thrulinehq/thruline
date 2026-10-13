//! M3.11.0 holdout-2, opened once at the M3.11.5 boundary and now burned.
//!
//! The first and only execution is recorded in A43/A44. Keep this ignored:
//! running a burned partition again cannot add generalization evidence.

use thruline_core::premise::{classify_outcome, Outcome};

#[derive(serde::Deserialize)]
struct Case {
    id: String,
    family: String,
    passed: Option<u32>,
    failed: Option<u32>,
    #[serde(default)]
    must_abstain: bool,
    grammar: String,
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
#[ignore = "BURNED after its one M3.11.5 opening; result recorded in A43/A44"]
fn sealed_holdout_2() {
    // A43 burn protection, mechanical. `#[ignore]` is NOT enough: `cargo test
    // -- --ignored` runs ignored tests, and that is exactly how this partition
    // was re-executed during M3.12.2's invariant sweep. A burned partition can
    // add no generalization evidence, so the body refuses unless a human names
    // it explicitly.
    if std::env::var("THRULINE_RUN_BURNED_HOLDOUT").as_deref() != Ok("yes") {
        println!("SKIPPED: burned partition. Set THRULINE_RUN_BURNED_HOLDOUT=yes to force.");
        return;
    }

    let raw = include_str!("fixtures/outcome-holdout-2.json");
    let h: Holdout = serde_json::from_str(raw).expect("holdout-2 parses");
    // A47: real and trap counters are SEPARATE. Sharing one `correct` and then
    // subtracting every trap only reports the truth when every trap passed —
    // with 3 of 16 traps misclassified this printed 71.4% where the direct
    // real-format count was 92.9%.
    let (mut real_correct, mut trap_correct) = (0usize, 0usize);
    let (mut abstained, mut wrong, mut real) = (0usize, 0usize, 0usize);
    let mut failures = Vec::new();

    for c in &h.cases {
        let (family, passed, failed) = observed(&classify_outcome(&c.text));
        if c.must_abstain {
            if family == "none" {
                trap_correct += 1;
            } else {
                wrong += 1;
                failures.push(format!(
                    "{} {}: trap parsed as {family} {passed:?}/{failed:?}",
                    c.id, c.grammar
                ));
            }
            continue;
        }
        real += 1;
        if family == "none" {
            abstained += 1;
        } else if family == c.family && passed == c.passed && failed == c.failed {
            real_correct += 1;
        } else {
            wrong += 1;
            failures.push(format!(
                "{} {}: expected {} {:?}/{:?}, got {family} {passed:?}/{failed:?}",
                c.id, c.grammar, c.family, c.passed, c.failed
            ));
        }
    }

    let traps = h.cases.len() - real;
    let coverage = 100.0 * real_correct as f64 / real.max(1) as f64;
    println!(
        "holdout-2: real={real_correct}/{real} = {coverage:.1}% \
         · traps {trap_correct}/{traps} abstained \
         · abstentions {abstained} · MISCLASSIFIED {wrong}"
    );
    assert_eq!(
        wrong,
        0,
        "holdout-2 wrong classifications:\n{}",
        failures.join("\n")
    );
    assert!(coverage >= 70.0, "holdout-2 coverage {coverage:.1}% < 70%");
}
