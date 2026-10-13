//! Shared production admission guard; the development Loom harness substitutes
//! only the atomic implementation, not this acquire/drop algorithm.
use super::{AtomicBool, Ordering};

pub(super) struct InFlightGuard<'a>(&'a AtomicBool);

impl<'a> InFlightGuard<'a> {
    pub(super) fn acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(flag))
    }
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
