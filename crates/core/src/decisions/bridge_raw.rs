//! Qualify bounded receipt bytes on a no-follow, nonblocking regular descriptor.
//! Regular-file I/O still depends on the filesystem/kernel; this is not a
//! universal OS wall-clock promise. FIFO/device/symlink substitution is inert.
use super::*;
#[cfg(unix)]
const LIMIT: usize = 32_768;
#[cfg(unix)]
fn qualified(metadata: &fs::Metadata, uid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.is_file()
        && metadata.uid() == uid
        && metadata.mode() & 0o077 == 0
        && metadata.len() <= LIMIT as u64
}
#[cfg(unix)]
fn same(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.uid() == right.uid()
        && left.mode() == right.mode()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}
#[cfg(unix)]
pub(super) fn read_private(path: &Path, uid: u32, deadline: Option<Instant>) -> Option<Vec<u8>> {
    use std::io::Read;
    if expired(deadline) {
        return None;
    }
    let named_before = fs::symlink_metadata(path).ok()?;
    if !qualified(&named_before, uid) {
        return None;
    }
    #[cfg(test)]
    tests::before_open(path);
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .ok()?;
    let file = fs::File::from(descriptor);
    let opened = file.metadata().ok()?;
    if !qualified(&opened, uid) || !same(&named_before, &opened) || expired(deadline) {
        return None;
    }
    #[cfg(test)]
    tests::after_open(path, &file);
    let mut raw = Vec::new();
    (&file)
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut raw)
        .ok()?;
    let descriptor_after = file.metadata().ok()?;
    let named_after = fs::symlink_metadata(path).ok()?;
    if raw.len() > LIMIT
        || expired(deadline)
        || !qualified(&descriptor_after, uid)
        || !qualified(&named_after, uid)
        || !same(&opened, &descriptor_after)
        || !same(&opened, &named_after)
    {
        None
    } else {
        Some(raw)
    }
}
#[cfg(not(unix))]
pub(super) fn read_private(_path: &Path, _uid: u32, _deadline: Option<Instant>) -> Option<Vec<u8>> {
    None
}
#[cfg(all(test, unix))]
#[path = "bridge_raw_tests.rs"]
mod tests;
