use thruline_core::decisions::checks_visual::visual_cache::{CaptureKey, ResultKey, VisualCache};
fn capture() -> CaptureKey {
    CaptureKey {
        implementation: "c28.7".into(),
        ui_sha256: "a".repeat(64),
        server_sha256: "b".repeat(64),
        server_mode: "closed-design".into(),
        capture_tool_sha256: "c".repeat(64),
        fixture_sha256: "d".repeat(64),
        renderer_sha256: "e".repeat(64),
        renderer_version: "Chrome/154.0.8037.98".into(),
        frame_width: 1512,
        frame_height: 920,
        capture_clock: "2026-10-06T23:42:17Z".into(),
        screen: "Inbox".into(),
        route: "#/design/Inbox".into(),
        scenario: "design".into(),
    }
}
#[test]
fn every_capture_input_invalidates_its_key() {
    let original = capture();
    let key = original.key();
    macro_rules! changes {
        ($field:ident,$value:expr) => {{
            let mut c = original.clone();
            c.$field = $value;
            assert_ne!(c.key(), key, stringify!($field));
        }};
    }
    changes!(implementation, "v8".into());
    changes!(ui_sha256, "1".repeat(64));
    changes!(server_sha256, "2".repeat(64));
    changes!(server_mode, "exact-backend".into());
    changes!(capture_tool_sha256, "3".repeat(64));
    changes!(fixture_sha256, "4".repeat(64));
    changes!(renderer_sha256, "5".repeat(64));
    changes!(renderer_version, "different".into());
    changes!(frame_width, 1600);
    changes!(frame_height, 2800);
    changes!(capture_clock, "2026-10-06T23:42:18Z".into());
    changes!(screen, "PR-Face".into());
    changes!(route, "#/design/Other".into());
    changes!(scenario, "fixed-record".into());
}
#[test]
fn accepted_revision_and_reference_invalidate_result_but_keep_capture() {
    let c = capture();
    let k = c.key();
    let original = ResultKey {
        capture_key: k.clone(),
        decision_id: "d1".into(),
        revision: 7,
        reference_sha256: "f".repeat(64),
        tolerance_bits: 0.5f64.to_bits(),
        compare_tool_sha256: "0".repeat(64),
    };
    let key = original.key();
    let mut changed = original.clone();
    changed.revision += 1;
    assert_ne!(changed.key(), key);
    assert_eq!(changed.capture_key, k);
    changed = original.clone();
    changed.decision_id = "d2".into();
    assert_ne!(changed.key(), key);
    changed = original.clone();
    changed.reference_sha256 = "a".repeat(64);
    assert_ne!(changed.key(), key);
    assert_eq!(changed.capture_key, k);
    changed = original.clone();
    changed.tolerance_bits = 0.6f64.to_bits();
    assert_ne!(changed.key(), key);
    changed = original;
    changed.compare_tool_sha256 = "b".repeat(64);
    assert_ne!(changed.key(), key);
}
#[test]
fn private_cache_reuses_verified_artifact_and_rejects_same_size_rewrite_and_symlink() {
    let temp = tempfile::tempdir().unwrap();
    let cache = VisualCache::at(temp.path().join("cache")).unwrap();
    let artifact = temp.path().join("pixels.png");
    std::fs::write(&artifact, b"abcd").unwrap();
    cache
        .save(
            "capture",
            "a".repeat(64).as_str(),
            &serde_json::json!({"verified":true}),
            std::slice::from_ref(&artifact),
        )
        .unwrap();
    let v: serde_json::Value = cache.load("capture", &"a".repeat(64)).unwrap();
    assert_eq!(v["verified"], true);
    std::fs::write(&artifact, b"wxyz").unwrap();
    assert!(cache
        .load::<serde_json::Value>("capture", &"a".repeat(64))
        .is_none());
    #[cfg(unix)]
    {
        std::fs::remove_file(&artifact).unwrap();
        std::os::unix::fs::symlink(temp.path().join("missing"), &artifact).unwrap();
        assert!(cache
            .load::<serde_json::Value>("capture", &"a".repeat(64))
            .is_none());
    }
}
#[test]
fn cache_identity_and_missing_artifacts_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let cache = VisualCache::at(temp.path().join("cache")).unwrap();
    let artifact = temp.path().join("pixels.png");
    std::fs::write(&artifact, b"abcd").unwrap();
    cache
        .save(
            "result",
            &"b".repeat(64),
            &serde_json::json!({"verified":true}),
            std::slice::from_ref(&artifact),
        )
        .unwrap();
    assert!(cache
        .load::<serde_json::Value>("result", &"c".repeat(64))
        .is_none());
    std::fs::remove_file(artifact).unwrap();
    assert!(cache
        .load::<serde_json::Value>("result", &"b".repeat(64))
        .is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            temp.path().join("cache"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert!(VisualCache::at(temp.path().join("cache")).is_none());
    }
}
