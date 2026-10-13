//! Explicit diagnostic capture of historical proposed C24 parameters on a
//! frozen scratch record. It creates no decision, acceptance or person act.
use std::path::PathBuf;
use thruline_core::decisions::checks_visual::{check_visual_at, visual_tool_identity};
use thruline_core::decisions::params::{parse_params, CheckParams};

#[test]
#[ignore = "explicit scratch-record benchmark requires pinned local Chrome"]
fn c28_all_fourteen_record_capture_baseline() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let params_path = PathBuf::from(
        std::env::var_os("THRULINE_VISUAL_BENCH_PARAMS")
            .expect("explicit diagnostic parameter file"),
    );
    let db_path = PathBuf::from(
        std::env::var_os("THRULINE_VISUAL_BENCH_DB").expect("explicit frozen scratch record"),
    );
    let version = std::env::var("THRULINE_VISUAL_BENCH_VERSION").expect("explicit exact version");
    let body = std::fs::read_to_string(params_path).unwrap();
    let CheckParams::VisualMatch(params) = parse_params("visual_match", 1, &body).unwrap() else {
        panic!("C1 parameters required")
    };
    assert_eq!(params.screens.len(), 14);
    assert!(params
        .screens
        .iter()
        .all(|screen| screen.mask_rectangles.is_empty()));
    let started = std::time::Instant::now();
    let tools = visual_tool_identity(&params);
    let outcome = check_visual_at(&repo, &version, &params, &db_path);
    println!(
        "{}",
        serde_json::json!({"authority": "diagnostic proposed parameters; no person acceptance created", "elapsed_ms": started.elapsed().as_millis(), "tools": tools, "outcome": outcome})
    );
    assert_eq!(outcome.visual.len(), 14);
    assert!(
        outcome
            .visual
            .iter()
            .all(|screen| screen.difference_pct.is_some()),
        "every screen must be measured: {outcome:?}"
    );
}
