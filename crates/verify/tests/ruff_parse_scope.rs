use serde_json::json;
use thruline_verify::integrations::decode;

#[test]
fn native_parse_failure_keeps_location_and_cannot_clear_selected_rules() {
    let diagnostics = json!([{"code":"invalid-syntax","filename":"/input/app.py",
        "message":"Expected ')', found ':'","location":{"row":1,"column":15},
        "end_location":{"row":1,"column":16},"fix":null}]);
    let mut capture = json!({"schema":"thruline.ruff-capture.v1","version":"0.16.7",
        "files":["/input/app.py"],"requested_files":["/input/app.py"],
        "rules":["F401","F841"],"diagnostics":diagnostics,
        "native_stdout":diagnostics.to_string(),"symlinks_not_followed":[],
        "inventory_exit":0,"scan_exit":1,"inventory_stderr":"","native_stderr":"",
        "configuration":"isolated explicit rule selection"});
    let report = decode("ruff-json-v1", &serde_json::to_vec(&capture).unwrap()).unwrap();
    assert!(!report.complete);
    assert_eq!(report.observations[0].rule, "invalid-syntax");
    assert_eq!(report.observations[0].line, Some(1));
    assert_eq!(report.facts["diagnostic_details"], diagnostics);
    assert!(report.limitations[0].starts_with("Ruff could not parse a selected source file."));
    assert!(report
        .limitations
        .iter()
        .any(|s| s.contains("Independent behavior checks")));

    // A known parser diagnostic does not authorize arbitrary unselected rules.
    capture["diagnostics"][0]["code"] = json!("UNREGISTERED999");
    capture["native_stdout"] = json!(capture["diagnostics"].to_string());
    assert!(decode("ruff-json-v1", &serde_json::to_vec(&capture).unwrap()).is_err());
}
