use serde_json::{json, Value};
use thruline_verify::integrations::decode;
fn report() -> Value {
    json!({"schemathesis_version":"4.26.1","complete":true,"stop_reason":"completed","operations":{"total":1,"selected":1,"tested":1,"skipped":0,"errored":0},"test_cases":{"generated":3,"with_failures":0,"without_checks":0},"failures":[],"errors":[]})
}
#[test]
fn partial_and_unasserted_api_runs_do_not_become_all_clear() {
    assert!(
        decode(
            "schemathesis-json-v1",
            &serde_json::to_vec(&report()).unwrap()
        )
        .unwrap()
        .complete
    );
    for (path, value) in [
        ("/complete", json!(false)),
        ("/stop_reason", json!("interrupted")),
        ("/operations/tested", json!(0)),
        ("/operations/skipped", json!(1)),
        ("/test_cases/without_checks", json!(1)),
    ] {
        let mut v = report();
        *v.pointer_mut(path).unwrap() = value;
        assert!(
            !decode("schemathesis-json-v1", &serde_json::to_vec(&v).unwrap())
                .unwrap()
                .complete,
            "{path}"
        );
    }
}
#[test]
fn unsupported_versions_and_contradictory_counts_are_rejected() {
    for (path, value) in [
        ("/schemathesis_version", json!("999.0")),
        ("/test_cases/with_failures", json!(9)),
        ("/operations/tested", json!(2)),
    ] {
        let mut v = report();
        *v.pointer_mut(path).unwrap() = value;
        assert!(decode("schemathesis-json-v1", &serde_json::to_vec(&v).unwrap()).is_err());
    }
    let raw = serde_json::to_vec(&report()).unwrap();
    for n in 0..raw.len() {
        assert!(decode("schemathesis-json-v1", &raw[..n]).is_err());
    }
}
