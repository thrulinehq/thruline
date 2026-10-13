//! Byte-space preflight for an already compiled native snapshot writer.

pub(super) fn native_upgrade_has_space(free_bytes: u64, snapshot_bytes: u64) -> bool {
    (15 * (1u64 << 30))
        .checked_add(snapshot_bytes)
        .is_some_and(|required| free_bytes >= required)
}
