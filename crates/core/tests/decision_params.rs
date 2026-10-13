//! C24: accepted decision parameters have a closed, versioned grammar.

use thruline_core::decisions::params::{parse_params, CheckParams, Surface, PARAMS_SCHEMA_VERSION};

#[test]
fn absence_params_keep_person_accepted_terms_surfaces_and_allowlist() {
    let raw = r#"{
      "kind":"text_absent",
      "terms":["obsolete"],
      "synonyms":[],
      "surfaces":["code","ui_copy","docs","tests"],
      "allowlist":["CHANGELOG.md"]
    }"#;
    let parsed = parse_params("text_absent", PARAMS_SCHEMA_VERSION, raw).expect("parse");
    let CheckParams::TextAbsent(params) = parsed else {
        panic!("expected text_absent");
    };
    assert_eq!(params.terms, vec!["obsolete"]);
    assert_eq!(
        params.surfaces,
        vec![
            Surface::Code,
            Surface::UiCopy,
            Surface::Docs,
            Surface::Tests
        ]
    );
    assert_eq!(params.allowlist, vec!["CHANGELOG.md"]);
    assert!(params.synonyms.is_empty());
}

#[test]
fn unknown_keys_are_errors_that_name_the_field() {
    let raw = r#"{"kind":"text_absent","terms":["obsolete"],"synonyms":[],"surfaces":["code"],"allowlist":[],"skip_comments":true}"#;
    let error = parse_params("text_absent", PARAMS_SCHEMA_VERSION, raw).expect_err("closed schema");
    assert!(error.to_string().contains("skip_comments"), "{error}");
}

#[test]
fn unknown_kind_or_schema_version_is_rejected() {
    let raw = r#"{"kind":"text_absent","terms":["obsolete"],"synonyms":[],"surfaces":["code"],"allowlist":[]}"#;
    assert!(parse_params("text_absent", PARAMS_SCHEMA_VERSION + 1, raw).is_err());
    assert!(parse_params("path_present", PARAMS_SCHEMA_VERSION, raw).is_err());
    assert!(parse_params(
        "mystery_check",
        PARAMS_SCHEMA_VERSION,
        r#"{"kind":"mystery_check"}"#
    )
    .is_err());
}

#[test]
fn every_phase_one_kind_has_a_closed_parameter_shape() {
    let cases = [
        (
            "symbol_absent",
            r#"{"kind":"symbol_absent","symbol":"OLD_FLAG","language":"rust","surfaces":["code"],"allowlist":[]}"#,
        ),
        (
            "ui_copy_absent",
            r#"{"kind":"ui_copy_absent","terms":["obsolete"],"allowlist":[]}"#,
        ),
        (
            "path_present",
            r#"{"kind":"path_present","path":"LICENSE","sha256":null}"#,
        ),
        (
            "test_present_and_runs",
            r#"{"kind":"test_present_and_runs","name_pattern":"can_open","command":"cargo test"}"#,
        ),
        (
            "feature_default_preserved",
            r#"{"kind":"feature_default_preserved","path":"src/lib.rs","symbol":"PAID_GATE","expected_value":"true"}"#,
        ),
        (
            "edits_within_paths",
            r#"{"kind":"edits_within_paths","allowed_paths":["src/**"]}"#,
        ),
        (
            "paths_untouched",
            r#"{"kind":"paths_untouched","paths":["tests/**"]}"#,
        ),
        (
            "term_everywhere",
            r#"{"kind":"term_everywhere","canonical":"v0.2","forbidden_variants":["0.2.0"],"surfaces":["ui_copy"],"allowlist":[]}"#,
        ),
        (
            "forbidden_words",
            r#"{"kind":"forbidden_words","words":["forbidden"],"surfaces":["ui_copy"],"allowlist":[]}"#,
        ),
        (
            "command_passes",
            r#"{"kind":"command_passes","command":"cargo test","is_test":true,"min_passed":1}"#,
        ),
    ];
    for (kind, raw) in cases {
        let parsed = parse_params(kind, PARAMS_SCHEMA_VERSION, raw).expect(kind);
        assert_eq!(parsed.kind(), kind);
    }
}
