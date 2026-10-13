//! Native upgrade writes a disposable snapshot; it does not invoke a compiler.
#[path = "../src/decisions/visual_upgrade_space.rs"]
mod upgrade_space;

use upgrade_space::native_upgrade_has_space;

const GIB: u64 = 1 << 30;

#[test]
fn an_empty_snapshot_still_reserves_the_internal_fifteen_gib_floor() {
    assert!(!native_upgrade_has_space(15 * GIB - 1, 0));
    assert!(native_upgrade_has_space(15 * GIB, 0));
}

#[test]
fn one_full_snapshot_rewrite_fits_at_the_exact_boundary_below_the_build_floor() {
    let snapshot = 9_277_509_632;
    let required = 15 * GIB + snapshot;
    assert!(required < 25 * GIB);
    assert!(native_upgrade_has_space(required, snapshot));
    assert!(!native_upgrade_has_space(required - 1, snapshot));
}

#[test]
fn a_small_snapshot_can_upgrade_without_twenty_five_gib_free() {
    assert!(native_upgrade_has_space(15 * GIB + 4096, 4096));
    assert!(!native_upgrade_has_space(15 * GIB + 4095, 4096));
}

#[test]
fn twenty_five_gib_cannot_cover_a_snapshot_larger_than_the_remaining_reserve() {
    assert!(!native_upgrade_has_space(25 * GIB, 10 * GIB + 1));
    assert!(native_upgrade_has_space(25 * GIB, 10 * GIB));
}

#[test]
fn one_byte_of_snapshot_space_is_reserved_in_addition_to_the_floor() {
    assert!(!native_upgrade_has_space(15 * GIB, 1));
    assert!(native_upgrade_has_space(15 * GIB + 1, 1));
}

#[test]
fn unrepresentable_floor_plus_snapshot_is_refused_even_at_maximum_free_space() {
    assert!(!native_upgrade_has_space(u64::MAX, u64::MAX));
    assert!(!native_upgrade_has_space(u64::MAX, u64::MAX - 15 * GIB + 1));
    assert!(native_upgrade_has_space(u64::MAX, u64::MAX - 15 * GIB));
}
