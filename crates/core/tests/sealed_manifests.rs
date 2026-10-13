//! §5h — every sealed partition's manifest is asserted against its contents.
//!
//! A47 is the reason this exists as a test rather than a convention. The
//! holdout-2 harness described its own result in prose and got it wrong (71.4%
//! reported, 92.9% actual), because a shared counter made the coverage formula
//! valid only when every trap passed. Prose cannot be checked; a manifest can.
//!
//! This runs in the default suite and reads only the fixtures' *metadata* — it
//! never invokes the classifier, so it cannot burn a sealed partition. That is
//! deliberate: the whole point is that seal integrity is checkable without
//! spending the one opening a holdout gets.

use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
struct Case {
    family: String,
    #[serde(default)]
    must_abstain: bool,
    #[serde(default)]
    provenance: String,
}

#[derive(serde::Deserialize)]
struct Manifest {
    total_cases: usize,
    real_cases: usize,
    trap_cases: usize,
    real_by_family: BTreeMap<String, usize>,
    /// holdout-3 onward: traps are grouped by which span they live in, because
    /// §5g's whole claim is that provenance decides. A manifest that did not
    /// record this could not show the traps actually span all three kinds.
    #[serde(default)]
    traps_by_provenance: BTreeMap<String, usize>,
}

#[derive(serde::Deserialize)]
struct Partition {
    manifest: Option<Manifest>,
    cases: Vec<Case>,
}

/// Every sealed partition file, with the name used in failure messages.
/// Adding a holdout without adding it here is itself a seal failure — the list
/// is short on purpose so the omission is visible in review.
const PARTITIONS: [(&str, &str); 3] = [
    ("holdout-1", include_str!("fixtures/outcome-holdout.json")),
    ("holdout-2", include_str!("fixtures/outcome-holdout-2.json")),
    ("holdout-3", include_str!("fixtures/outcome-holdout-3.json")),
];

fn verify(name: &str, raw: &str) {
    let p: Partition =
        serde_json::from_str(raw).unwrap_or_else(|e| panic!("{name} does not parse: {e}"));
    let m = p
        .manifest
        .unwrap_or_else(|| panic!("{name} has no manifest block — §5h requires one"));

    let real: Vec<&Case> = p.cases.iter().filter(|c| !c.must_abstain).collect();
    let traps = p.cases.len() - real.len();
    let mut by_family: BTreeMap<String, usize> = BTreeMap::new();
    for c in &real {
        *by_family.entry(c.family.clone()).or_default() += 1;
    }

    assert_eq!(
        m.total_cases,
        p.cases.len(),
        "{name}: manifest says {} cases, file has {}",
        m.total_cases,
        p.cases.len()
    );
    assert_eq!(
        m.real_cases,
        real.len(),
        "{name}: manifest says {} real cases, file has {}",
        m.real_cases,
        real.len()
    );
    assert_eq!(
        m.trap_cases, traps,
        "{name}: manifest says {} traps, file has {traps}",
        m.trap_cases
    );
    assert_eq!(
        m.real_by_family, by_family,
        "{name}: manifest's per-family real counts disagree with contents"
    );
    if !m.traps_by_provenance.is_empty() {
        let mut tp: BTreeMap<String, usize> = BTreeMap::new();
        for c in p.cases.iter().filter(|c| c.must_abstain) {
            *tp.entry(c.provenance.clone()).or_default() += 1;
        }
        assert_eq!(
            m.traps_by_provenance, tp,
            "{name}: manifest's traps-by-provenance disagrees with contents"
        );
    }
    assert_eq!(
        m.real_cases + m.trap_cases,
        m.total_cases,
        "{name}: manifest does not add up internally"
    );
    println!(
        "  {name}: {} real ({:?}) + {} traps = {} — manifest verified",
        m.real_cases, m.real_by_family, m.trap_cases, m.total_cases
    );
}

#[test]
fn every_sealed_partition_manifest_matches_its_contents() {
    for (name, raw) in PARTITIONS {
        verify(name, raw);
    }
}
