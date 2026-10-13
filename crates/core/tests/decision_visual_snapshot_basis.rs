use thruline_core::decisions::checks_visual::visual_tool_identity;
use thruline_core::decisions::params::VisualMatchParams;

#[test]
fn replay_basis_names_exact_snapshot_and_native_write_reserve() {
    let params = VisualMatchParams {
        tolerance_pct: 0.1,
        screens: Vec::new(),
    };
    let identity = visual_tool_identity(&params);
    assert_eq!(
        identity["build_flags"]["record_snapshot"],
        "internal-apfs-exact-bytes-v1"
    );
    assert_eq!(
        identity["build_flags"]["native_upgrade_reserve"],
        "15-gib-plus-complete-snapshot-bytes"
    );
    assert_eq!(identity["visual_impl"], "c1-pixel-v9-pinned-apfs-snapshot");
}
