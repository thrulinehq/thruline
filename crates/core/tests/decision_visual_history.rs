use std::path::Path;
use thruline_core::decisions::checks_visual::visual_cache::VisualCache;
use thruline_core::decisions::checks_visual::visual_history::{
    load_captures, preserve_build, save_captures, HistoricalInput, HistoricalScreen,
};
use thruline_core::decisions::checks_visual::CaptureReceipt;
fn input() -> HistoricalInput {
    HistoricalInput {
        implementation: "history-test-v1".into(),
        commit_oid: "1".repeat(40),
        tree_hash: "2".repeat(40),
        capture_tool_sha256: "3".repeat(64),
        fixture_sha256: "4".repeat(64),
        renderer_sha256: "5".repeat(64),
        renderer_framework_sha256: "6".repeat(64),
        renderer_version: "Chrome/fixture-v1".into(),
        capture_clock: "2026-10-06T23:42:17Z".into(),
        screens: vec![HistoricalScreen {
            screen: "Inbox".into(),
            route: "/#/recorded".into(),
            scenario: "fixed-record".into(),
            legacy_route: Some("/#/old-recorded".into()),
            legacy_scenario: Some("old-record".into()),
            width: 1512,
            height: 920,
        }],
    }
}
fn receipt(input: &HistoricalInput, png: &Path) -> CaptureReceipt {
    let s = &input.screens[0];
    CaptureReceipt {
        screen: s.screen.clone(),
        route: s.route.clone(),
        scenario: s.scenario.clone(),
        actual_route: s.legacy_route.clone(),
        actual_scenario: s.legacy_scenario.clone(),
        commit_oid: input.commit_oid.clone(),
        tree_hash: input.tree_hash.clone(),
        renderer_sha256: input.renderer_sha256.clone(),
        renderer_version: Some(input.renderer_version.clone()),
        capture_clock: Some(input.capture_clock.clone()),
        fixture_sha256: Some(input.fixture_sha256.clone()),
        frame_width: Some(s.width),
        frame_height: Some(s.height),
        content_width: Some(s.width),
        content_height: Some(s.height),
        overflow_x: Some(false),
        overflow_y: Some(false),
        scroll_regions: vec![],
        clipped_regions: vec![],
        housekeeping_responses: vec![],
        route_found: true,
        screenshot_path: Some(png.into()),
        failure: None,
    }
}
#[test]
fn historical_build_and_pixels_survive_disposable_source_and_rewrites_invalidate_reuse() {
    let t = tempfile::tempdir().unwrap();
    let cache = VisualCache::at(t.path().join("cache")).unwrap();
    let source = t.path().join("source");
    std::fs::create_dir_all(source.join("dist/assets")).unwrap();
    std::fs::write(source.join("dist/index.html"), b"exact old index").unwrap();
    std::fs::write(source.join("dist/assets/app.js"), b"exact old ui").unwrap();
    std::fs::write(source.join("server"), b"exact old server").unwrap();
    std::fs::write(source.join("pixels.png"), b"exact measured pixels").unwrap();
    let build = preserve_build(&cache, &source.join("dist"), &source.join("server")).unwrap();
    assert_eq!(build.ui_sha256.len(), 64);
    assert_eq!(build.server_sha256.len(), 64);
    let i = input();
    save_captures(
        &cache,
        &i,
        &build,
        &[receipt(&i, &source.join("pixels.png"))],
    )
    .unwrap();
    std::fs::remove_dir_all(&source).unwrap();
    let reused = load_captures(&cache, &i).expect("exact old replay remains cached");
    assert_eq!(reused[0].actual_route.as_deref(), Some("/#/old-recorded"));
    let png = reused[0].screenshot_path.as_ref().unwrap();
    assert_eq!(std::fs::read(png).unwrap(), b"exact measured pixels");
    std::fs::write(png, b"different measured px").unwrap();
    assert!(load_captures(&cache, &i).is_none());
}
#[test]
fn changed_old_server_or_any_bound_capture_input_requires_a_fresh_replay() {
    let t = tempfile::tempdir().unwrap();
    let cache = VisualCache::at(t.path().join("cache")).unwrap();
    let dist = t.path().join("dist");
    std::fs::create_dir(&dist).unwrap();
    std::fs::write(dist.join("index.html"), b"html").unwrap();
    let server = t.path().join("server");
    let png = t.path().join("pixels.png");
    std::fs::write(&server, b"server").unwrap();
    std::fs::write(&png, b"pixels").unwrap();
    let build = preserve_build(&cache, &dist, &server).unwrap();
    let i = input();
    save_captures(&cache, &i, &build, &[receipt(&i, &png)]).unwrap();
    macro_rules! changed {
        ($field:ident,$value:expr) => {{
            let mut c = i.clone();
            c.$field = $value;
            assert!(load_captures(&cache, &c).is_none(), stringify!($field));
        }};
    }
    changed!(commit_oid, "7".repeat(40));
    changed!(tree_hash, "8".repeat(40));
    changed!(fixture_sha256, "9".repeat(64));
    changed!(renderer_framework_sha256, "a".repeat(64));
    changed!(capture_clock, "2026-10-06T23:42:18Z".into());
    changed!(implementation, "v2".into());
    let mut c = i.clone();
    c.screens[0].height += 1;
    assert!(load_captures(&cache, &c).is_none());
    c = i.clone();
    c.screens[0].legacy_route = Some("/#/other".into());
    assert!(load_captures(&cache, &c).is_none());
    std::fs::write(&build.files[0], b"changed build").unwrap();
    assert!(load_captures(&cache, &i).is_none());
}
#[test]
fn failed_missing_or_mismatched_receipts_never_populate_a_history_cache() {
    let t = tempfile::tempdir().unwrap();
    let cache = VisualCache::at(t.path().join("cache")).unwrap();
    let dist = t.path().join("dist");
    std::fs::create_dir(&dist).unwrap();
    std::fs::write(dist.join("index.html"), b"html").unwrap();
    let server = t.path().join("server");
    let png = t.path().join("pixels.png");
    std::fs::write(&server, b"server").unwrap();
    std::fs::write(&png, b"pixels").unwrap();
    let build = preserve_build(&cache, &dist, &server).unwrap();
    let i = input();
    let r = receipt(&i, &png);
    let mut c = r.clone();
    c.failure = Some("capture timeout".into());
    assert!(save_captures(&cache, &i, &build, &[c]).is_err());
    c = r.clone();
    c.fixture_sha256 = Some("0".repeat(64));
    assert!(save_captures(&cache, &i, &build, &[c]).is_err());
    c = r;
    c.screenshot_path = None;
    assert!(save_captures(&cache, &i, &build, &[c]).is_err());
    assert!(load_captures(&cache, &i).is_none());
}
