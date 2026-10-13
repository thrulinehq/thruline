//! C3.0 — the capsule-source pin (A173).
//!
//! Holds the probe artifact to its own arithmetic and, more importantly, holds
//! the two facts that decide what C3 may build: `verified` and the attested-run
//! tables are measured zeros, and the `unsupported` class is shadow. A capsule
//! commit that quietly starts filling those sections fails here first.
//!
//! The gate: **no capsule code exists in core before its schema and compiler
//! land with the rulings recorded.** It evolves into the compiler's containment
//! test exactly as RB0's no-detection pin became RB1's shadow gate.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn artifact() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/recon/c30-capsule-sources.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read the C3.0 artifact"))
        .expect("the C3.0 artifact is JSON")
}

#[test]
fn every_section_states_a_source_and_an_availability() {
    let a = artifact();
    let sections = a["sections"].as_object().expect("sections");
    assert!(
        sections.len() >= 9,
        "the probe covered only {} sections",
        sections.len()
    );
    for (name, row) in sections {
        assert!(
            row["source"].as_str().is_some_and(|s| s.len() > 10),
            "{name} names no source"
        );
        let avail = row["availability"].as_str().unwrap_or_default();
        assert!(
            matches!(
                avail,
                "provable" | "partial" | "absent" | "shadow-blocked" | "provable-as-inference"
            ),
            "{name} has an unrecognised availability {avail:?}"
        );
        assert!(
            row["note"].as_str().is_some_and(|n| n.len() > 20),
            "{name} carries no note — an availability without a reason is a vibe"
        );
    }
}

#[test]
fn the_two_measured_zeros_that_bound_c3_are_recorded() {
    let a = artifact();
    // `verified` has never graduated and has never been produced. A capsule
    // section titled 'verified' would name a class this machine cannot fill.
    assert_eq!(
        a["corpus_claim_states"]["verified"],
        Value::Null,
        "a `verified` node appeared corpus-wide; C3's verified section must be re-measured, \
         not quietly filled"
    );
    // Every capture/attested table empty: the proof-grade half of do-not-repeat
    // has no rows at all on this machine.
    let attested = a["attested_run_tables"]
        .as_object()
        .expect("attested tables");
    assert!(!attested.is_empty(), "the attested-table census vanished");
    let total: u64 = attested.values().filter_map(Value::as_u64).sum();
    assert_eq!(
        total, 0,
        "attested runs appeared; do-not-repeat's proof-grade half must be re-measured"
    );
}

#[test]
fn the_shadow_class_finding_survives_in_the_artifact() {
    let a = artifact();
    let caution = &a["sections"]["caution_no_proof"];
    assert_eq!(
        caution["availability"], "shadow-blocked",
        "the CAUTION section's shadow finding was softened without a ruling"
    );
    assert_eq!(
        caution["beta"],
        Value::from(0),
        "beta grew an unsupported claim; the measured-zero half of the finding moved"
    );
    // The corpus rows exist but sit in one root — both halves matter, so both
    // are pinned.
    assert_eq!(a["corpus_claim_states"]["unsupported"], Value::from(60));
}

#[test]
fn the_scope_finding_names_its_numbers() {
    let a = artifact();
    let scope = &a["scope_finding"];
    assert!(
        scope["astrology_project_rows"].as_u64().unwrap_or(0) >= 10,
        "the fragmentation finding lost its count"
    );
    assert!(
        scope["rule"]
            .as_str()
            .is_some_and(|r| r.contains("never guessed")),
        "the no-guessing rule is the whole point of the scope finding"
    );
    // beta resolves, Astrology does not — the contrast is the finding.
    assert_eq!(
        a["specimens"]["beta"]["repo_identity"]["resolves"],
        Value::Bool(true)
    );
    assert_eq!(
        a["specimens"]["astrology"]["repo_identity"]["resolves"],
        Value::Bool(false)
    );
    assert_eq!(
        a["specimens"]["astrology"]["repo_identity"]["basis"],
        "unresolvable"
    );
}

#[test]
fn capsule_code_is_contained_to_its_module_and_the_named_loop() {
    // Evolved at A181, exactly as RB0's no-detection pin became RB1's shadow
    // gate and A170's no-sensor pin became sensor containment. The pre-C3.1
    // form banned capsule code from core outright; the capsule now exists, so
    // the gate is containment: the compiler lives in `capsule.rs`, and only
    // the recovery loop's own modules may compile one. A surface that starts
    // minting capsules somewhere else fails here first.
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    const MAY_COMPILE: &[&str] = &["capsule.rs", "recovery_demo.rs"];
    let mut walked = 0usize;
    for entry in fs::read_dir(&src).expect("walk core src") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let text = fs::read_to_string(&path).expect("read source");
        if !MAY_COMPILE.contains(&name.as_str()) {
            assert!(
                !text.contains("capsule::compile"),
                "{name} compiles a capsule — only {MAY_COMPILE:?} may"
            );
        }
        // The types stay declared in one place regardless.
        if name != "capsule.rs" {
            assert!(
                !text.contains("pub struct Capsule "),
                "{name} declares a second Capsule type"
            );
        }
        walked += 1;
    }
    assert!(walked > 20, "the walk read only {walked} files");
}

#[test]
fn the_probes_own_pre_capsule_claim_stays_true_about_its_own_moment() {
    // A173 shipped zero capsule code and the artifact says so. That claim is
    // about A173's moment and stays true forever — it is not a claim about
    // today, and rewriting it to match today would be editing history.
    let a = artifact();
    assert_eq!(a["capsule_code_written"], Value::from(0));
}
