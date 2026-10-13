//! Explicit production-adapter performance experiment. It creates no person
//! act or accepted decision. Reference changes are private proposed inputs.
use std::path::{Path, PathBuf};
use std::time::Instant;
use thruline_core::decisions::checks::CheckOutcome;
use thruline_core::decisions::checks_visual::visual_inputs::file_digest;
use thruline_core::decisions::checks_visual::{check_visual_at_for_decision, visual_tool_identity};
use thruline_core::decisions::params::{parse_params, CheckParams, VisualMatchParams};
fn run(
    repo: &Path,
    version: &str,
    params: &VisualMatchParams,
    db: &Path,
    evidence: &Path,
    name: &str,
    revision: i64,
) -> (CheckOutcome, u128, serde_json::Value) {
    let profile = evidence.join(format!("{name}-stages.jsonl"));
    std::fs::write(&profile, "").unwrap();
    std::env::set_var("THRULINE_VISUAL_PROFILE", &profile);
    let started = Instant::now();
    let tools = visual_tool_identity(params);
    let preflight_ms = started.elapsed().as_millis();
    let outcome = check_visual_at_for_decision(
        repo,
        version,
        params,
        db,
        "diagnostic/c28-speed-14",
        revision,
    );
    let elapsed_ms = started.elapsed().as_millis();
    let report = serde_json::json!({"authority":"diagnostic proposed parameters; no accepted decision or person act; original record never written","version":version,"diagnostic_revision":revision,"preflight_ms":preflight_ms,"elapsed_ms":elapsed_ms,"tools":tools,"outcome":outcome});
    std::fs::write(
        evidence.join(format!("{name}-report.json")),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{name}: {elapsed_ms} ms incl {preflight_ms} ms preflight; {} screens; {:?}",
        outcome.visual.len(),
        outcome.state
    );
    assert_eq!(outcome.visual.len(), 14);
    assert!(
        outcome.visual.iter().all(|s| s.difference_pct.is_some()),
        "all14 must be measured; see private report"
    );
    (outcome, elapsed_ms, tools)
}
#[test]
#[ignore = "explicit scratch-record 90/1/15 production capture experiment"]
fn c28_production_fourteen_cold_unchanged_and_one_reference_input() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let evidence = PathBuf::from(
        std::env::var_os("THRULINE_VISUAL_SPEED_EVIDENCE").expect("private performance evidence"),
    );
    let runtime = PathBuf::from(
        std::env::var_os("THRULINE_VISUAL_CACHE_DIR")
            .expect("fresh runtime cache outside evidence"),
    );
    assert!(
        !runtime.exists(),
        "a first-run measurement must use a fresh cache path"
    );
    std::fs::create_dir_all(&runtime).unwrap();
    let refs = runtime.join("reference-root");
    let folder = refs.join("docs/hq-next/reference");
    std::fs::create_dir_all(&folder).unwrap();
    let db = PathBuf::from(std::env::var_os("THRULINE_VISUAL_BENCH_DB").unwrap());
    let version = std::env::var("THRULINE_VISUAL_BENCH_VERSION").unwrap();
    let body =
        std::fs::read_to_string(std::env::var_os("THRULINE_VISUAL_BENCH_PARAMS").unwrap()).unwrap();
    let CheckParams::VisualMatch(mut params) = parse_params("visual_match", 1, &body).unwrap()
    else {
        panic!()
    };
    assert_eq!(params.screens.len(), 14);
    for s in &params.screens {
        std::fs::copy(repo.join(&s.reference_path), refs.join(&s.reference_path)).unwrap();
    }
    std::env::set_var("THRULINE_VISUAL_REFERENCE_ROOT", &refs);
    std::env::set_var("THRULINE_VISUAL_DIFF_DIR", runtime.join("diffs"));
    let (first, cold, tools) = run(&repo, &version, &params, &db, &evidence, "cold", 0);
    let (second, warm, _) = run(&repo, &version, &params, &db, &evidence, "unchanged", 0);
    assert_eq!(
        first, second,
        "unchanged cached verdict bytes stay identical"
    );
    let reference = refs.join(&params.screens[0].reference_path);
    let status=std::process::Command::new("python3").args(["-c","from PIL import Image; import sys; p=sys.argv[1]; im=Image.open(p).convert('RGBA'); im.putpixel((0,0),(255,0,0,255)); im.save(p)"]).arg(&reference).status().unwrap();
    assert!(status.success());
    params.screens[0].reference_sha256 = file_digest(&reference).unwrap();
    std::fs::write(
        evidence.join("one-reference-proposed-params.json"),
        serde_json::to_vec_pretty(&params).unwrap(),
    )
    .unwrap();
    let (_, one, _) = run(&repo, &version, &params, &db, &evidence, "one-reference", 1);
    let summary = serde_json::json!({"basis":"exact named version + recorded checker tool identity; frozen original scratch record manifest; private proposed reference image changes one Inbox pixel and diagnostic revision 0→1; all14 result keys change, unchanged capture/comparison substages reused","version":version,"tools":tools,"cold_ms":cold,"unchanged_ms":warm,"one_reference_ms":one,"budgets_ms":[90000,1000,15000],"passes":[cold<=90000,warm<=1000,one<=15000]});
    std::fs::write(
        evidence.join("summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    println!("{summary}");
    assert!(cold <= 90000, "first14 exceeds90s");
    assert!(warm <= 1000, "unchanged exceeds1s");
    assert!(one <= 15000, "one reference input exceeds15s");
}
