use serde_json::Value;
use thruline_verify::integrations::decode;
const STRONG: &[u8] = include_bytes!("fixtures/mutation-modes/package-strong.json");
#[test]
fn real_package_feature_and_shard_outcomes_retain_native_phase_evidence() {
    for data in [
        STRONG,
        include_bytes!("fixtures/mutation-modes/workspace.json"),
        include_bytes!("fixtures/mutation-modes/shard-0.json"),
    ] {
        let report = decode("cargo-mutants-json-v1", data).unwrap();
        assert!(report.complete, "{:?}", report.limitations);
        assert_eq!(report.facts["missed"], 0);
        assert_eq!(report.facts["mutation_baseline"][0]["summary"], "Success");
        assert!(report.facts["mutation_outcomes"].as_array().unwrap().len() > 1);
    }
}
#[test]
fn real_weak_and_equivalent_survivors_remain_missed() {
    for (data, missed) in [
        (
            include_bytes!("fixtures/mutation-modes/weak-default.json").as_slice(),
            5,
        ),
        (
            include_bytes!("fixtures/mutation-modes/equivalent.json").as_slice(),
            1,
        ),
    ] {
        let report = decode("cargo-mutants-json-v1", data).unwrap();
        assert!(report.complete);
        assert_eq!(report.facts["missed"], missed);
    }
}
#[test]
fn actual_missing_runner_and_feature_baselines_remain_incomplete() {
    for data in [
        include_bytes!("fixtures/mutation-modes/nextest-missing.json").as_slice(),
        include_bytes!("fixtures/mutation-modes/missing-feature.json").as_slice(),
    ] {
        let report = decode("cargo-mutants-json-v1", data).unwrap();
        assert!(!report.complete);
        assert_eq!(report.facts["total_mutants"], 0);
    }
}
#[test]
fn missing_baseline_duplicate_mutants_and_falsely_caught_builds_are_incomplete() {
    let original: Value = serde_json::from_slice(STRONG).unwrap();
    let mut absent = original.clone();
    absent["outcomes"].as_array_mut().unwrap().remove(0);
    let mut duplicate = original.clone();
    duplicate["outcomes"][2] = duplicate["outcomes"][1].clone();
    let mut failed_build = original.clone();
    failed_build["outcomes"][1]["phase_results"][0]["process_status"] =
        serde_json::json!({"Failure":101});
    let mut count = original.clone();
    count["caught"] = serde_json::json!(4);
    for value in [absent, duplicate, failed_build, count] {
        let report = decode(
            "cargo-mutants-json-v1",
            &serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert!(!report.complete);
    }
}

#[test]
fn actual_nextest_failure_100_catches_mutants_and_success_keeps_survivors() {
    for (data, missed) in [
        (
            include_bytes!("fixtures/mutation-modes/nextest-strong.json").as_slice(),
            0,
        ),
        (
            include_bytes!("fixtures/mutation-modes/nextest-weak.json").as_slice(),
            5,
        ),
    ] {
        let report = decode("cargo-mutants-json-v1", data).unwrap();
        assert!(report.complete, "{:?}", report.limitations);
        assert_eq!(report.facts["missed"], missed);
    }
}
