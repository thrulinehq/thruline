use super::*;
fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/integrations/fixtures/knip")
        .join(format!("{name}.json"));
    parse_json(&std::fs::read(path).unwrap()).unwrap()
}
fn report(value: &Value) -> Result<Report, DecodeError> {
    decode(&serde_json::to_vec(value).unwrap())
}
#[test]
fn retains_named_families_positions_and_duplicate_aliases() {
    let result = report(&fixture("families-defective")).unwrap();
    assert!(!result.complete);
    assert_eq!(result.facts["finding_count"], 3);
    let duplicate = result
        .observations
        .iter()
        .find(|o| o.rule == "duplicates")
        .unwrap();
    assert_eq!(duplicate.line, Some(1));
    assert!(duplicate.message.contains("sum ↔ aliasSum"));
    assert_eq!(
        result.facts["knip_issues"][1]["duplicates"][0][1]["line"],
        2
    );
    assert!(result
        .observations
        .iter()
        .any(|o| o.rule == "unlisted" && o.message.contains("minimist") && o.line == Some(2)));
    assert!(result
        .observations
        .iter()
        .any(|o| o.rule == "unresolved" && o.message.contains("./missing.js")));
}
#[test]
fn corrected_behavior_fixture_has_no_candidates_but_retains_scope_limits() {
    let result = report(&fixture("families-correct")).unwrap();
    assert!(result.complete);
    assert_eq!(result.facts["finding_count"], 0);
    assert!(result
        .limitations
        .iter()
        .any(|s| s.contains("dynamic loading")));
}
#[test]
fn nonexistent_entry_and_excluded_compilers_are_incomplete_even_with_equal_counts() {
    for name in ["families-incomplete", "compiler-excluded"] {
        let result = report(&fixture(name)).unwrap();
        assert!(!result.complete);
        assert_eq!(
            result.facts["scan_coverage"]["counters"]["processed"],
            result.facts["scan_coverage"]["counters"]["total"]
        );
    }
}
#[test]
fn catalog_namespaces_and_missing_reference_locations_are_distinct() {
    let result = report(&fixture("catalog-references")).unwrap();
    assert!(!result.complete);
    assert_eq!(result.facts["knip_issue_counts"]["catalogReferences"], 4);
    assert_eq!(result.facts["knip_issue_counts"]["catalog"], 2);
    assert!(result
        .observations
        .iter()
        .any(|o| o.rule == "catalogReferences"
            && o.message.contains("backend.express")
            && o.line == Some(5)));
    assert!(report(&fixture("catalog-correct")).unwrap().complete);
}
#[test]
fn workspace_selection_and_strict_production_keep_actual_scope() {
    let ordinary = report(&fixture("upstream-workspaces")).unwrap();
    let strict = report(&fixture("workspaces-strict")).unwrap();
    let selected = report(&fixture("workspace-selection")).unwrap();
    assert_eq!(ordinary.facts["finding_count"], 1);
    assert_eq!(strict.facts["finding_count"], 2);
    assert_eq!(
        strict.facts["knip_scope"]["native_options"]["isProduction"],
        true
    );
    assert_eq!(
        selected.facts["knip_scope"]["selected_workspaces"],
        json!(["packages/consumer"])
    );
    assert_eq!(
        selected.facts["knip_native_issues"][0]["workspace"],
        "packages/consumer"
    );
}
#[test]
fn missing_source_map_cannot_clear_and_native_compiler_does_not_claim_build_success() {
    for name in [
        "mapping-references-correct",
        "mapping-exact-links",
        "upstream-compilers",
    ] {
        let result = report(&fixture(name)).unwrap();
        assert!(result.complete);
        assert!(result.facts["knip_scope"]["compiler_boundary"]
            .as_str()
            .unwrap()
            .contains("do not establish unused-export"));
        assert!(result.facts["knip_scope"]["source_mapping_boundary"]
            .as_str()
            .unwrap()
            .contains("do not identify every mapping"));
    }
    let result = report(&fixture("mapping-references-incomplete")).unwrap();
    assert!(!result.complete);
    assert!(result
        .observations
        .iter()
        .any(|o| o.rule == "unresolved" && o.message.contains("#worker")));
}
#[test]
fn legacy_reporter_is_readable_without_inventing_workspace_or_group_spans() {
    let result = report(&fixture("legacy-families")).unwrap();
    assert_eq!(result.facts["finding_count"], 3);
    assert!(!result.facts.contains_key("knip_native_issues"));
}
#[test]
fn contradictory_native_metadata_is_refused() {
    for pointer in [
        "/nativeIssues/0/name",
        "/nativeIssues/0/file",
        "/nativeIssues/2/symbols/0/name",
    ] {
        let mut value = fixture("families-defective");
        *value.pointer_mut(pointer).unwrap() = json!("different-owned-symbol");
        assert!(report(&value).is_err(), "{pointer}");
    }
    let mut value = fixture("families-defective");
    value["coverage"]["counters"]["unlisted"] = json!(0);
    assert!(report(&value).is_err());
    let mut value = fixture("families-defective");
    value["issues"][1]["duplicates"][0][0]["line"] = json!(0);
    assert!(report(&value).is_err());
    assert!(decode(br#"{"issues":[],"issues":[{}]}"#).is_err());
}

#[test]
fn malformed_workspace_scope_is_refused_before_ui_projection() {
    for (key, value) in [
        ("selectedWorkspaces", json!("not-an-array")),
        ("includedWorkspaceDirs", json!([3])),
        (
            "nativeOptions",
            json!({"isProduction":"yes","configFilePath":null}),
        ),
    ] {
        let mut native = fixture("families-correct");
        native["coverage"][key] = value;
        assert!(report(&native).is_err());
    }
}
