//! C2/C4 accepted parameters are closed and name exact, local evidence.

use serde_json::{json, Value};
use thruline_core::decisions::params::{parse_params, CheckParams, PARAMS_SCHEMA_VERSION};

fn parse(kind: &str, value: &Value) -> thruline_core::error::Result<CheckParams> {
    parse_params(kind, PARAMS_SCHEMA_VERSION, &value.to_string())
}

#[test]
fn c2_accepts_distinct_project_files_and_optional_half_open_spans() {
    let basic = json!({"kind":"file_equals","left":"docs/left.txt","right":"docs/right.txt"});
    let parsed = parse("file_equals", &basic).expect("optional span");
    assert_eq!(
        serde_json::from_str::<Value>(&parsed.json().expect("json")).expect("value"),
        json!({"kind":"file_equals","left":"docs/left.txt","right":"docs/right.txt","span":null})
    );
    let selected = json!({"kind":"file_equals","left":"docs/left.txt","right":"docs/right.txt",
        "span":{"left":{"start":2,"end":9},"right":null}});
    assert!(parse("file_equals", &selected).is_ok());
}

#[test]
fn c2_rejects_tautologies_unsafe_paths_empty_spans_and_unknown_fields() {
    let base =
        json!({"kind":"file_equals","left":"docs/left.txt","right":"docs/right.txt","span":null});
    for (field, invalid) in [
        ("right", json!("docs/left.txt")),
        ("left", json!("../website/readme.txt")),
        ("left", json!("/tmp/readme.txt")),
        ("left", json!("docs/./left.txt")),
        ("right", json!("docs//right.txt")),
    ] {
        let mut value = base.clone();
        value[field] = invalid;
        assert!(parse("file_equals", &value).is_err(), "{field}: {value}");
    }
    for span in [
        json!({"left":null,"right":null}),
        json!({"left":{"start":5,"end":5},"right":null}),
        json!({"left":{"start":9,"end":5},"right":null}),
    ] {
        let mut value = base.clone();
        value["span"] = span;
        assert!(parse("file_equals", &value).is_err(), "{value}");
    }
    let mut unknown = base;
    unknown["skip_whitespace"] = json!(true);
    assert!(parse("file_equals", &unknown).is_err());
}

#[test]
fn c4_requires_a_recorded_binding_digest_and_pinned_local_golden() {
    let base = json!({"kind":"golden_output",
        "command_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "golden_path":"docs/golden.txt",
        "golden_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"});
    assert!(parse("golden_output", &base).is_ok());
    for (field, invalid) in [
        ("command_id", json!("cargo test")),
        ("golden_path", json!("../website/golden.txt")),
        ("golden_sha256", json!("not-a-digest")),
        (
            "golden_sha256",
            json!("BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"),
        ),
    ] {
        let mut value = base.clone();
        value[field] = invalid;
        assert!(parse("golden_output", &value).is_err(), "{field}: {value}");
    }
    let mut unknown = base;
    unknown["shell"] = json!("echo success");
    assert!(parse("golden_output", &unknown).is_err());
}
