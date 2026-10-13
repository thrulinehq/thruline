//! §6.3's coverage clause, in both directions.

use std::collections::{BTreeMap, BTreeSet};

use thruline_verify::coverage::{
    changed_line_coverage, parse_cobertura, parse_lcov, Coverage, DEFAULT_MIN_PCT,
};
use thruline_verify::verdict::Severity;

fn changed(path: &str, lines: &[u32]) -> BTreeMap<String, BTreeSet<u32>> {
    let mut m = BTreeMap::new();
    m.insert(path.to_string(), lines.iter().copied().collect());
    m
}

const LCOV: &str = "TN:\nSF:src/shipping.py\nDA:1,1\nDA:2,1\nDA:3,0\nDA:4,0\nend_of_record\n";

#[test]
fn a_missing_artifact_is_not_measured_and_is_never_zero_percent() {
    // The failure this must not make: most repositories collect no coverage,
    // and reporting them at 0% would refute every honest claim in all of them.
    let c = changed_line_coverage(&changed("src/a.py", &[1, 2]), &BTreeMap::new());
    assert!(matches!(c, Coverage::NotMeasured { .. }), "{c:?}");
    let f = c.finding(DEFAULT_MIN_PCT).expect("a finding");
    assert_eq!(
        f.severity,
        Severity::Info,
        "a missing artifact refuted a claim"
    );
    assert_eq!(f.detector, "coverage_not_measured");
}

#[test]
fn coverage_is_computed_over_changed_executable_lines_only() {
    let hits = parse_lcov(LCOV);
    // Lines 1-2 covered, 3-4 not; line 99 is a blank the report never mentions
    // and must not count as uncovered.
    let c = changed_line_coverage(&changed("src/shipping.py", &[1, 2, 3, 99]), &hits);
    match c {
        Coverage::Measured {
            changed_executable,
            covered,
            pct_bp,
        } => {
            assert_eq!(
                changed_executable, 3,
                "a line the tool never mentioned was counted"
            );
            assert_eq!(covered, 2);
            assert_eq!(pct_bp, 6666);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn low_coverage_warns_and_good_coverage_says_nothing() {
    let hits = parse_lcov(LCOV);
    let low = changed_line_coverage(&changed("src/shipping.py", &[3, 4]), &hits);
    let f = low.finding(DEFAULT_MIN_PCT).expect("a finding");
    assert_eq!(f.severity, Severity::Warn);
    assert_eq!(f.detector, "low_changed_line_coverage");

    let good = changed_line_coverage(&changed("src/shipping.py", &[1, 2]), &hits);
    assert!(
        good.finding(DEFAULT_MIN_PCT).is_none(),
        "fully covered changed lines produced a finding"
    );
}

#[test]
fn an_artifact_that_names_none_of_the_changed_files_is_not_measured() {
    // A stale report is not evidence of zero coverage.
    let hits = parse_lcov(LCOV);
    let c = changed_line_coverage(&changed("src/billing.py", &[1, 2]), &hits);
    assert!(matches!(c, Coverage::NotMeasured { .. }), "{c:?}");
}

#[test]
fn paths_are_matched_by_suffix_because_reports_and_diffs_disagree_about_prefixes() {
    let hits = parse_lcov("SF:/build/ci/src/shipping.py\nDA:1,1\nend_of_record\n");
    let c = changed_line_coverage(&changed("src/shipping.py", &[1]), &hits);
    assert!(matches!(c, Coverage::Measured { covered: 1, .. }), "{c:?}");
}

#[test]
fn cobertura_is_parsed_as_well_as_lcov() {
    let xml = "<coverage><packages><package><classes>\
               <class filename=\"src/shipping.py\"><lines>\
               <line number=\"1\" hits=\"3\"/><line number=\"2\" hits=\"0\"/>\
               </lines></class></classes></package></packages></coverage>";
    let hits = parse_cobertura(xml);
    let c = changed_line_coverage(&changed("src/shipping.py", &[1, 2]), &hits);
    match c {
        Coverage::Measured {
            changed_executable,
            covered,
            pct_bp,
        } => {
            assert_eq!((changed_executable, covered, pct_bp), (2, 1, 5000));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_percentage_is_an_integer_because_hashed_payloads_carry_no_floats() {
    // BUILD-PLAN C.4/N0 and D2: basis points, so nothing here can ever put an
    // IEEE-754 double into a payload that gets canonicalised.
    let hits = parse_lcov(LCOV);
    let c = changed_line_coverage(&changed("src/shipping.py", &[1, 2, 3]), &hits);
    let json = format!("{c:?}");
    assert!(json.contains("pct_bp: 6666"), "{json}");
    assert!(
        !json.contains('.'),
        "a float reached the coverage result: {json}"
    );
}
