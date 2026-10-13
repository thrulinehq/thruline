//! C1's accepted visual contract belongs to each named screen.

use serde_json::{json, Value};
use thruline_core::decisions::params::{parse_params, CheckParams, PARAMS_SCHEMA_VERSION};

fn visual() -> Value {
    json!({
        "kind": "visual_match",
        "tolerance_pct": 0.5,
        "screens": [
            {
                "name": "Inbox",
                "route": "/#/design/Inbox",
                "scenario": "design",
                "legacy_route": "#/inbox",
                "legacy_scenario": "fixed-record",
                "reference_path": "docs/hq-next/reference/Inbox.png",
                "reference_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "width": 1512,
                "height": 920,
                "full_page": false,
                "mask_rectangles": []
            },
            {
                "name": "Whole-Story",
                "route": "/#/design/Whole-Story",
                "scenario": "design",
                "reference_path": "docs/hq-next/reference/Whole-Story.png",
                "reference_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "width": 1280,
                "height": 720,
                "full_page": true,
                "mask_rectangles": [{"x": 12, "y": 20, "width": 150, "height": 32}]
            }
        ]
    })
}

fn parse(value: &Value) -> thruline_core::error::Result<CheckParams> {
    parse_params("visual_match", PARAMS_SCHEMA_VERSION, &value.to_string())
}

#[test]
fn c1_round_trips_distinct_screen_scenarios_viewports_capture_modes_and_masks() {
    let expected = visual();
    let parsed = parse(&expected).expect("closed visual parameters");
    assert_eq!(parsed.kind(), "visual_match");
    let actual: Value = serde_json::from_str(&parsed.json().expect("serialize"))
        .expect("serialized visual parameters");
    assert_eq!(actual, expected);
    let CheckParams::VisualMatch(params) = parsed else {
        panic!("visual kind");
    };
    assert_eq!(params.screens.len(), 2);
    assert_eq!(params.screens[0].viewport_width, 1512);
    assert_eq!(params.screens[1].viewport_width, 1280);
    assert_eq!(params.screens[1].mask_rectangles.len(), 1);
}

#[test]
fn c1_requires_every_capture_dimension_on_each_screen() {
    for field in [
        "scenario",
        "width",
        "height",
        "full_page",
        "mask_rectangles",
    ] {
        let mut value = visual();
        value["screens"][0]
            .as_object_mut()
            .expect("screen object")
            .remove(field);
        assert!(parse(&value).is_err(), "missing {field} must fail");
    }
    let mut old_global_shape = visual();
    old_global_shape["width"] = json!(1512);
    assert!(parse(&old_global_shape).is_err(), "unknown global viewport");
}

#[test]
fn c1_rejects_unknown_capture_fields_and_unpinned_screen_identity() {
    for (place, field) in [("screen", "ignore_diff"), ("mask", "automatic")] {
        let mut value = visual();
        match place {
            "screen" => value["screens"][0][field] = json!(true),
            "mask" => value["screens"][1]["mask_rectangles"][0][field] = json!(true),
            _ => unreachable!(),
        }
        let error = parse(&value).expect_err("closed capture schema");
        assert!(error.to_string().contains(field), "{error}");
    }
    for (field, invalid) in [
        ("name", json!("")),
        ("scenario", json!("")),
        ("scenario", json!("model_inferred")),
        ("width", json!(0)),
        ("height", json!(0)),
        ("reference_sha256", json!("not-a-sha256")),
    ] {
        let mut value = visual();
        value["screens"][0][field] = invalid;
        assert!(parse(&value).is_err(), "invalid {field} must fail");
    }
}

#[test]
fn c1_rejects_empty_or_out_of_viewport_mask_rectangles() {
    for rectangle in [
        json!({"x": 0, "y": 0, "width": 0, "height": 20}),
        json!({"x": 0, "y": 0, "width": 20, "height": 0}),
        json!({"x": 1500, "y": 0, "width": 20, "height": 20}),
        json!({"x": 0, "y": 910, "width": 20, "height": 20}),
    ] {
        let mut value = visual();
        value["screens"][0]["mask_rectangles"] = json!([rectangle]);
        assert!(parse(&value).is_err(), "invalid rectangle must fail");
    }
}

#[test]
fn c1_routes_stay_on_the_local_capture_origin() {
    for route in [
        "https://example.invalid/Inbox",
        "//example.invalid/Inbox",
        "javascript:alert(1)",
        "../Inbox",
        "/#/design/../Admin",
        "/\\example.invalid/Inbox",
        "/#/design/Inbox\n",
    ] {
        let mut value = visual();
        value["screens"][0]["route"] = json!(route);
        assert!(parse(&value).is_err(), "unsafe route {route:?} must fail");
    }
}

#[test]
fn c1_accepts_only_person_pinned_local_legacy_routes_and_closed_scenarios() {
    let expected = visual();
    assert_eq!(
        serde_json::from_str::<Value>(&parse(&expected).expect("aliases").json().expect("json"))
            .expect("value"),
        expected
    );
    for route in [
        "https://example.invalid/inbox",
        "//example.invalid/inbox",
        "../inbox",
    ] {
        let mut value = visual();
        value["screens"][0]["legacy_route"] = json!(route);
        assert!(parse(&value).is_err(), "unsafe legacy route {route:?}");
    }
    for scenario in ["design", "inferred", "javascript:run()"] {
        let mut value = visual();
        value["screens"][0]["legacy_scenario"] = json!(scenario);
        assert!(
            parse(&value).is_err(),
            "unaccepted legacy scenario {scenario:?}"
        );
    }
    let mut dangling = visual();
    dangling["screens"][0]
        .as_object_mut()
        .expect("screen")
        .remove("legacy_route");
    assert!(
        parse(&dangling).is_err(),
        "scenario without route cannot run"
    );
    let mut dangling = visual();
    dangling["screens"][0]
        .as_object_mut()
        .expect("screen")
        .remove("legacy_scenario");
    assert!(
        parse(&dangling).is_err(),
        "route without scenario cannot run"
    );
}
