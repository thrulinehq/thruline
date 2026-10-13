//! Capture-only, explicitly diagnostic serial baseline over exact built design
//! artifacts. No production backend or full-load claim is made by this test.
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use thruline_core::decisions::checks_visual::visual_inputs::load_fixture_manifest;
use thruline_core::decisions::checks_visual::{check_visual_with, CaptureReceipt, VisualRuntime};
use thruline_core::decisions::params::{parse_params, CheckParams};
struct Kill(Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn git(repo: &std::path::Path, object: &str) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", object])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().into()
}
#[test]
#[ignore = "explicit pinned Chrome serial capture-only diagnostic"]
fn c28_serial_design_capture_baseline() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let evidence = PathBuf::from(
        std::env::var_os("THRULINE_VISUAL_SERIAL_EVIDENCE").expect("private diagnostic evidence"),
    );
    let dist = PathBuf::from(
        std::env::var_os("THRULINE_VISUAL_BENCH_DIST").expect("exact built diagnostic dist"),
    );
    let version = std::env::var("THRULINE_VISUAL_BENCH_VERSION").expect("exact version");
    let commit = git(&repo, &format!("{version}^{{commit}}"));
    let tree = git(&repo, &format!("{commit}^{{tree}}"));
    let body =
        std::fs::read_to_string(std::env::var_os("THRULINE_VISUAL_BENCH_PARAMS").unwrap()).unwrap();
    let CheckParams::VisualMatch(params) = parse_params("visual_match", 1, &body).unwrap() else {
        panic!()
    };
    assert_eq!(params.screens.len(), 14);
    assert!(params
        .screens
        .iter()
        .all(|s| s.scenario == "design" && s.mask_rectangles.is_empty()));
    let total = std::time::Instant::now();
    let began = std::time::Instant::now();
    let frozen = load_fixture_manifest(&PathBuf::from(
        std::env::var_os("THRULINE_VISUAL_BENCH_DB").unwrap(),
    ))
    .unwrap();
    let fixture_ms = began.elapsed().as_millis();
    let server_code="from http.server import ThreadingHTTPServer,SimpleHTTPRequestHandler\nfrom functools import partial\nimport sys\ns=ThreadingHTTPServer(('127.0.0.1',0),partial(SimpleHTTPRequestHandler,directory=sys.argv[1]))\nprint(s.server_port,flush=True)\ns.serve_forever()";
    let began = std::time::Instant::now();
    let mut server = Kill(
        Command::new("python3")
            .args(["-u", "-c", server_code])
            .arg(&dist)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut port = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut port)
        .unwrap();
    let origin = format!("http://127.0.0.1:{}", port.trim().parse::<u16>().unwrap());
    let server_ms = began.elapsed().as_millis();
    let jobs:Vec<_>=params.screens.iter().enumerate().map(|(index,s)| {
        let png=std::fs::read(repo.join(&s.reference_path)).unwrap();assert_eq!(sha(&png),s.reference_sha256);
        let width=u32::from_be_bytes(png[16..20].try_into().unwrap());let height=u32::from_be_bytes(png[20..24].try_into().unwrap());
        serde_json::json!({"screen":s.screen,"route":s.route,"scenario":s.scenario,"width":width,"height":height,"full_page":false,"output":evidence.join("shots").join(format!("{index}.png"))})
    }).collect();
    let profile = evidence.join("capture-stages.jsonl");
    std::fs::write(&profile, "").unwrap();
    let input = evidence.join("input.json");
    std::fs::write(&input,serde_json::json!({"origin":origin,"renderer":PathBuf::from(std::env::var_os("THRULINE_VISUAL_RENDERER").unwrap()),"capture_clock":frozen.manifest.capture_clock,"fixture_sha256":frozen.manifest_sha256,"profile_path":profile,"jobs":jobs}).to_string()).unwrap();
    let script = evidence.join("serial-capture.mjs");
    let script_bytes =
        std::fs::read(repo.join("crates/core/src/decisions/visual_capture.mjs")).unwrap();
    std::fs::write(&script, &script_bytes).unwrap();
    let began = std::time::Instant::now();
    let result = Command::new("node")
        .arg(&script)
        .arg(&input)
        .output()
        .unwrap();
    let capture_ms = began.elapsed().as_millis();
    std::fs::write(evidence.join("capture.stderr.log"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let shots: Vec<serde_json::Value> = serde_json::from_slice(&result.stdout).unwrap();
    std::fs::write(evidence.join("shots.json"), &result.stdout).unwrap();
    assert_eq!(shots.len(), 14);
    let captures = shots
        .into_iter()
        .zip(params.screens.iter())
        .map(|(shot, s)| {
            let mut bound = shot.as_object().unwrap().clone();
            let error = bound.remove("error").unwrap();
            bound.insert("failure".into(), error);
            bound.insert("route".into(), serde_json::json!(s.route));
            bound.insert("scenario".into(), serde_json::json!(s.scenario));
            bound.insert("actual_route".into(), serde_json::json!(s.route));
            bound.insert("actual_scenario".into(), serde_json::json!(s.scenario));
            bound.insert("commit_oid".into(), serde_json::json!(commit));
            bound.insert("tree_hash".into(), serde_json::json!(tree));
            bound.insert(
                "renderer_sha256".into(),
                serde_json::json!(std::env::var("THRULINE_VISUAL_RENDERER_SHA256").unwrap()),
            );
            bound.insert("route_found".into(), serde_json::json!(true));
            serde_json::from_value::<CaptureReceipt>(bound.into()).unwrap()
        })
        .collect();
    let runtime = VisualRuntime {
        reference_root: repo.clone(),
        renderer_sha256: std::env::var("THRULINE_VISUAL_RENDERER_SHA256").ok(),
        renderer_version: None,
        capture_clock: Some(frozen.manifest.capture_clock),
        fixture_sha256: Some(frozen.manifest_sha256),
        captures,
        diff_root: Some(evidence.join("diffs")),
    };
    let began = std::time::Instant::now();
    let report = check_visual_with(&repo, &version, &params, &runtime);
    let diff_ms = began.elapsed().as_millis();
    let basis = serde_json::json!({"authority":"capture-only diagnostic proposed parameters; no acceptance/person act; no full backend load claim","server_mode":"diagnostic-static-design-exact-dist","commit_oid":commit,"tree_hash":tree,"server_script_sha256":sha(server_code.as_bytes()),"capture_script_sha256":sha(&script_bytes),"fixture_verify_ms":fixture_ms,"server_start_ms":server_ms,"capture_ms":capture_ms,"diff_ms":diff_ms,"experiment_total_ms":total.elapsed().as_millis(),"report":report});
    std::fs::write(
        evidence.join("report.json"),
        serde_json::to_vec_pretty(&basis).unwrap(),
    )
    .unwrap();
    println!("{}", basis);
    assert!(
        report.screens.iter().all(|s| s.difference_pct.is_some()),
        "every screen must actually be compared"
    );
}
