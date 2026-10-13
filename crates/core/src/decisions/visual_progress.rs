//! Scoped progress transport. Progress is diagnostic and never changes a verdict.
use std::cell::RefCell;
use std::io::Write;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualProgress {
    pub completed: usize,
    pub total: usize,
    pub label: String,
}
pub type VisualProgressObserver = Arc<dyn Fn(VisualProgress) + Send + Sync>;
thread_local! { static OBSERVER: RefCell<Option<VisualProgressObserver>> = const { RefCell::new(None) }; }
struct RestoreObserver(Option<VisualProgressObserver>);
impl Drop for RestoreObserver {
    fn drop(&mut self) {
        OBSERVER.with(|slot| {
            *slot.borrow_mut() = self.0.take();
        });
    }
}

pub fn with_visual_progress_observer<T>(
    observer: VisualProgressObserver,
    work: impl FnOnce() -> T,
) -> T {
    let previous = OBSERVER.with(|slot| slot.borrow_mut().replace(observer));
    let _restore = RestoreObserver(previous);
    work()
}
pub(super) fn current_observer() -> Option<VisualProgressObserver> {
    OBSERVER.with(|slot| slot.borrow().clone())
}
pub(super) fn emit_to(
    observer: Option<&VisualProgressObserver>,
    completed: usize,
    total: usize,
    label: String,
) {
    if completed > total || total > 64 {
        return;
    }
    let _ = writeln!(std::io::stderr(), "{label}");
    if let Some(observer) = observer {
        let progress = VisualProgress {
            completed,
            total,
            label,
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(progress)));
    }
}
pub(super) fn emit(completed: usize, total: usize, label: String) {
    emit_to(current_observer().as_ref(), completed, total, label);
}
