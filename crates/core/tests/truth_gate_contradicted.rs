//! The negative truth gate: `contradicted` requires a conflicting receipt.
//!
//! NORTHSTAR-v2 makes `contradicted` the only strong accusation, and the brief
//! (§5d verdict authority, §5f evidence authority, §5q closed-world conviction)
//! makes it structural: absence of evidence must NEVER produce a negative
//! verdict. Until now that invariant was enforced by review. Review does not
//! fail a build.
//!
//! Three parts, in increasing distance from the authority:
//!
//!   A. Exhaustive behavioural — drive `contract::check` over the whole bounded
//!      cross-product of claim and evidence shapes and assert that every
//!      `Contradicted` it returns is backed by an actual conflicting receipt.
//!   B. The absence rule stated on its own, so a failure names the doctrine it
//!      broke rather than a combinatorial index.
//!   C. Structural tripwire — pin the number of accusation construction sites
//!      in the PRODUCTION region of the authority files, so a new one cannot be
//!      added silently. The cut between production and test code is COMPUTED
//!      (first `#[cfg(test)]` immediately followed by `mod`), not hardcoded: an
//!      earlier draft of this gate cut `derive.rs` at a `#[cfg(test)] fn` and
//!      left 1,754 production lines — including the sole verdict authority —
//!      outside the region it claimed to protect.

use thruline_core::contract::{
    check, Check, ClaimAssertion, ClaimPolarity, EvidenceOutcome, Family,
};

/// The ONLY thing that licenses an accusation: the recorded run itself reported
/// failure. Anything else — missing counts, absent evidence, an unrecognised
/// family — is at most inconclusive.
fn evidence_actually_conflicts(ev: &EvidenceOutcome) -> bool {
    ev.failed.is_some_and(|f| f > 0) || ev.ok == Some(false)
}

fn claim_domain() -> Vec<ClaimAssertion> {
    let mut out = Vec::new();
    for family in [Family::Test, Family::Build, Family::None] {
        for passed in [None, Some(0), Some(1), Some(33)] {
            for failed in [None, Some(0), Some(1)] {
                for all_pass in [true, false] {
                    for polarity in [
                        ClaimPolarity::AssertsSuccess,
                        ClaimPolarity::AssertsFailure,
                        ClaimPolarity::Uncertain,
                    ] {
                        out.push(ClaimAssertion {
                            family,
                            passed,
                            failed,
                            all_pass,
                            polarity,
                        });
                    }
                }
            }
        }
    }
    out
}

fn evidence_domain() -> Vec<EvidenceOutcome> {
    let mut out = Vec::new();
    for family in [Family::Test, Family::Build, Family::None] {
        for passed in [None, Some(0), Some(1), Some(33)] {
            for failed in [None, Some(0), Some(1)] {
                for ok in [None, Some(true), Some(false)] {
                    out.push(EvidenceOutcome {
                        family,
                        passed,
                        failed,
                        ok,
                    });
                }
            }
        }
    }
    out
}

/// A. No input anywhere in the bounded domain may produce an accusation that
///    the evidence does not license.
#[test]
fn contradicted_never_outruns_its_receipt() {
    let claims = claim_domain();
    let evidence = evidence_domain();
    let mut accusations = 0usize;

    for c in &claims {
        for ev in &evidence {
            if let Check::Contradicted(reason) = check(c, ev) {
                accusations += 1;
                assert!(
                    evidence_actually_conflicts(ev),
                    "ACCUSATION WITHOUT A RECEIPT — the cardinal sin.\n\
                     claim: {c:?}\nevidence: {ev:?}\nreason given: {reason}\n\
                     No failure was recorded (failed={:?}, ok={:?}), so this verdict \
                     accuses on absence. See NORTHSTAR-v2 and brief §5f.",
                    ev.failed,
                    ev.ok
                );
                // The accusation must also be within family — a build result may
                // never contradict a test claim (§5p evidence families).
                assert_eq!(
                    c.family, ev.family,
                    "cross-family accusation: claim {:?} vs evidence {:?}",
                    c.family, ev.family
                );
                assert_ne!(
                    c.polarity,
                    ClaimPolarity::Uncertain,
                    "uncertain polarity must never convict (§5q): {c:?}"
                );
            }
        }
    }

    // Guard against the vacuous pass: if a refactor made `Contradicted`
    // unreachable, every assertion above would hold and the gate would go
    // quietly green while the product lost its only accusation.
    assert!(
        accusations > 0,
        "no Contradicted verdict was reachable across {} claim x {} evidence \
         combinations — the gate has become vacuous and is no longer testing anything",
        claims.len(),
        evidence.len()
    );
}

/// B. The absence rule, stated alone so a regression names the doctrine.
#[test]
fn absence_of_evidence_is_never_an_accusation() {
    // Evidence that recorded nothing: no failure count, no ok flag.
    for family in [Family::Test, Family::Build, Family::None] {
        for passed in [None, Some(0), Some(33)] {
            let silent = EvidenceOutcome {
                family,
                passed,
                failed: None,
                ok: None,
            };
            for c in claim_domain() {
                let verdict = check(&c, &silent);
                assert!(
                    !matches!(verdict, Check::Contradicted(_)),
                    "silent evidence produced an accusation.\nclaim: {c:?}\n\
                     evidence: {silent:?}\nverdict: {verdict:?}\n\
                     'ThruLine cannot currently prove this' is the only honest \
                     verdict here; a false accusation costs more than several \
                     missed warnings."
                );
            }
        }
    }

    // And a run that passed cleanly cannot convict an asserts-success claim.
    let clean = EvidenceOutcome {
        family: Family::Test,
        passed: Some(33),
        failed: Some(0),
        ok: Some(true),
    };
    let claim = ClaimAssertion {
        family: Family::Test,
        passed: Some(33),
        failed: Some(0),
        all_pass: true,
        polarity: ClaimPolarity::AssertsSuccess,
    };
    assert!(!matches!(check(&claim, &clean), Check::Contradicted(_)));
}

/// The production/test boundary: the first `#[cfg(test)]` that is immediately
/// followed by a `mod` declaration. Returns the number of source lines before
/// it. A `#[cfg(test)]` on a bare `fn` does NOT end the production region.
fn production_region(src: &str) -> usize {
    let lines: Vec<&str> = src.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let next = lines[i + 1..].iter().find(|l| !l.trim().is_empty());
        if next.is_some_and(|l| l.trim_start().starts_with("mod ")) {
            return i;
        }
    }
    lines.len()
}

fn count_in_production(path: &str, needle: &str) -> usize {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let cut = production_region(&src);
    src.lines().take(cut).filter(|l| l.contains(needle)).count()
}

/// C. Structural tripwire. These numbers are not sacred — they are a review
///    prompt. If you added or removed an accusation site on purpose, update the
///    count IN THE SAME COMMIT that explains why, and re-run part A.
#[test]
fn accusation_construction_sites_are_pinned() {
    // contract.rs: the two guarded returns — the all-pass-vs-failures conflict
    // (Family::Test) and the reported build failure (Family::Build).
    assert_eq!(
        count_in_production("src/contract.rs", "Check::Contradicted"),
        2,
        "the number of `Check::Contradicted` sites in production contract.rs changed. \
         Every one of them must be reachable only when the recorded run reported \
         failure. Re-review, then update this pin with the reason."
    );

    // derive.rs: display mapping, dimension projection, the sort key, the sole
    // authority behind `contradiction_eligibility`, and the conviction veto.
    assert_eq!(
        count_in_production("src/derive.rs", "NodeState::Contradicted"),
        10,
        "the number of `NodeState::Contradicted` mentions in production derive.rs \
         changed. The authority is the one behind `contradiction_eligibility`; a new \
         mention that bypasses it is the failure this pin exists to catch."
    );
    // 4 -> 5 at A94b, reviewed and accepted: the new mention is the
    // `"contradicted" => Some(EvidenceState::Contradicted)` arm of
    // `EvidenceState::parse`, which reads a value BACK from the persisted
    // projection. It cannot manufacture an accusation, because the only string
    // it can be handed came from the `evidence_state` column, whose CHECK
    // domain is written solely by the authority this gate already pins. A parse
    // is a decoder, not a producer — and it abstains (`None`) rather than
    // defaulting, so a corrupt value fails loud instead of arriving as a verdict.
    assert_eq!(
        count_in_production("src/derive.rs", "EvidenceState::Contradicted"),
        5,
        "the number of `EvidenceState::Contradicted` mentions in production derive.rs \
         changed. Evidence state is projected FROM node state; a new independent \
         producer would let the evidence dimension accuse on its own."
    );
}

/// R17a-3 — the promotion side of the same tripwire.
///
/// The gate above pins ACCUSATION sites. `EvidenceState::Verified` now has
/// exactly one producer (`dimensions_for_claim`'s attested arm), and a
/// promotion added silently is as dangerous as an accusation added silently —
/// arguably more so, because a false accusation is argued with and a false
/// promotion is believed.
///
/// Reviewed at 5 production mentions in `derive.rs`, each classified:
///   186 — the A87 applicability rule CONSUMING verified to derive stale
///   215 — `parse`, a decoder from the CHECK-constrained column
///   307 — THE PRODUCER: the only place a claim becomes verified
///   433 — the RULING 4 composition cap, which removes verified
///   455 — the sort-key collapse, a consumer
/// Exactly one produces. If this count changes, find the new site and classify
/// it before touching the number.
#[test]
fn verified_promotion_sites_are_pinned() {
    assert_eq!(
        count_in_production("src/derive.rs", "EvidenceState::Verified"),
        5,
        "the number of `EvidenceState::Verified` mentions in production derive.rs changed. \
         Exactly ONE of them may produce a verified verdict, and it must require an \
         affirmative `check_attested_run` against an accepted assured pair. Classify the \
         new site before updating this pin."
    );
}

/// The boundary detector itself is load-bearing, so it gets a test. A
/// `#[cfg(test)]` on a function must not be mistaken for the module boundary —
/// that mistake is what left the first draft of this gate blind.
#[test]
fn production_boundary_ignores_cfg_test_functions() {
    let src = "fn a() {}\n#[cfg(test)]\nfn helper() {}\nfn b() {}\n#[cfg(test)]\nmod tests {\n}\n";
    assert_eq!(
        production_region(src),
        4,
        "cut must land on the mod, not the fn"
    );

    let none = "fn a() {}\nfn b() {}\n";
    assert_eq!(
        production_region(none),
        2,
        "no test module means all production"
    );

    let blank_between = "fn a() {}\n#[cfg(test)]\n\nmod tests {\n}\n";
    assert_eq!(
        production_region(blank_between),
        1,
        "a blank line between the attribute and the mod must still be recognised"
    );
}

// ---------------------------------------------------------------------------
// D. The fixture half of the gate (A90).
//
// Held back when this gate first shipped, because it would have been red: the
// only committed fixture, `ui/public/fixtures/project.json`, presented two
// nodes as `contradicted` with `accusation: true`, `evidence_count: 0` and
// `evidence: []`. That artifact is deleted; these two assertions are what stop
// it coming back.
// ---------------------------------------------------------------------------

/// The states that carry an accusation, per `derive::NodeState::is_accusation`
/// (`derive.rs:90`). `dropped` counts: it is an accusation class, and it is
/// full-shadow diagnostics-only besides.
const ACCUSING_STATES: [&str; 2] = ["contradicted", "dropped"];

fn fixture_dir() -> std::path::PathBuf {
    // Integration tests run with CWD = the crate root.
    std::path::Path::new("../../ui/public/fixtures").to_path_buf()
}

fn json_files_under(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            json_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

/// D1. No fixture may present an accusation it has no evidence for.
///
/// On a fresh clone this scans zero files, because every fixture path is
/// gitignored — so the count is asserted and reported rather than left to look
/// like a pass. On a developer machine it scans the real exports, which is
/// where it has teeth.
#[test]
fn no_fixture_presents_an_accusation_without_evidence() {
    let mut files = Vec::new();
    json_files_under(&fixture_dir(), &mut files);
    files.sort();

    let mut nodes_checked = 0usize;
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
            panic!("fixture {} is not valid JSON", path.display());
        };
        let Some(nodes) = doc.get("nodes").and_then(|n| n.as_array()) else {
            continue; // index.json and friends carry no nodes.
        };
        let receipts = doc.get("receipts");
        for node in nodes {
            nodes_checked += 1;
            let state = node.get("state").and_then(|s| s.as_str()).unwrap_or("");
            let accusing = ACCUSING_STATES.contains(&state)
                || node.get("accusation").and_then(|a| a.as_bool()) == Some(true);
            if !accusing {
                continue;
            }
            let id = node.get("id").and_then(|i| i.as_str()).unwrap_or("<no id>");
            let evidence_count = node
                .get("evidence_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let receipt_evidence = receipts
                .and_then(|r| r.get(id))
                .and_then(|r| r.get("evidence"))
                .and_then(|e| e.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            assert!(
                evidence_count > 0 || receipt_evidence > 0,
                "ACCUSATION WITHOUT A RECEIPT, in a fixture.\n  file: {}\n  node: {id}\n  \
                 state: {state}\n  evidence_count: {evidence_count}, receipt evidence: \
                 {receipt_evidence}\nAn unsupported finding must never wear the accusation \
                 badge — see NORTHSTAR-v2 and A90.",
                path.display()
            );
        }
    }
    println!(
        "scanned {} fixture file(s), {nodes_checked} node(s)",
        files.len()
    );
}

/// D2. Fixtures are generated, never hand-authored — so none may be tracked.
///
/// This is the rule A90 established, and unlike D1 it is never vacuous: it
/// holds on a fresh clone and on a developer machine alike. A hand-written
/// fixture is an unreceipted claim about the product's own output, and it
/// drifts silently — which is exactly how `project.json` came to ship two
/// accusations with empty evidence against a contract it no longer matched.
#[test]
fn no_fixture_is_committed_to_the_repo() {
    let out = std::process::Command::new("git")
        .args(["ls-files", "ui/public/fixtures"])
        .current_dir("../..")
        .output();
    let Ok(out) = out else {
        return; // No git available; D1 still ran.
    };
    assert!(out.status.success(), "git ls-files failed");
    let tracked: Vec<&str> = std::str::from_utf8(&out.stdout)
        .unwrap_or("")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(
        tracked.is_empty(),
        "fixtures must be generated by `thruline export`, never committed. \
         Tracked: {tracked:?}\nExport writes real transcript content; a committed \
         fixture is both a privacy hazard and a claim that goes stale unnoticed (A90)."
    );
}
