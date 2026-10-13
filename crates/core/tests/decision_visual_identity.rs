//! The approved launcher/product cannot reapprove a changed framework.
#[cfg(unix)]
#[test]
fn unchanged_approved_renderer_reuses_identity_but_changed_same_product_framework_refuses() {
    use std::os::unix::fs::PermissionsExt;
    use thruline_core::decisions::checks_visual::visual_cache::VisualCache;
    use thruline_core::decisions::checks_visual::visual_identity::verify_renderer_with_framework;
    use thruline_core::decisions::checks_visual::visual_inputs::file_digest;
    let temp = tempfile::tempdir().unwrap();
    let contents = temp.path().join("Chrome.app/Contents");
    let launcher = contents.join("MacOS/Google Chrome");
    std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    std::fs::write(
        &launcher,
        b"#!/bin/sh\nprintf 'Google Chrome 154.0.8037.98\\n'\n",
    )
    .unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    let framework = contents.join(
        "Frameworks/Google Chrome Framework.framework/Versions/Current/Google Chrome Framework",
    );
    std::fs::create_dir_all(framework.parent().unwrap()).unwrap();
    std::fs::write(&framework, b"approved framework").unwrap();
    let approved = file_digest(&launcher).unwrap();
    let approved_framework = file_digest(&framework).unwrap();
    let cache = VisualCache::at(temp.path().join("cache")).unwrap();
    let first = verify_renderer_with_framework(
        &launcher,
        &approved,
        &approved,
        "Chrome/154.0.8037.98",
        Some(&approved_framework),
        &cache,
    )
    .unwrap();
    let second = verify_renderer_with_framework(
        &launcher,
        &approved,
        &approved,
        "Chrome/154.0.8037.98",
        Some(&approved_framework),
        &cache,
    )
    .unwrap();
    assert_eq!(first.sha256, second.sha256);
    assert_eq!(first.product, second.product);
    std::fs::write(&framework, b"modified framework").unwrap();
    assert!(
        verify_renderer_with_framework(
            &launcher,
            &approved,
            &approved,
            "Chrome/154.0.8037.98",
            Some(&approved_framework),
            &cache
        )
        .is_err(),
        "same approved product/launcher cannot self-pin changed framework bytes"
    );
}
