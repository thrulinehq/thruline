use std::path::Path;
use std::sync::{Arc, Mutex};
use thruline_core::decisions::checks_visual::{
    check_visual_at, with_visual_progress_observer, VisualProgressObserver,
};
use thruline_core::decisions::params::{parse_params, CheckParams};

fn unavailable_check() {
    let body = serde_json::json!({"kind":"visual_match","tolerance_pct":0.5,"screens":[{"name":"fixture","route":"#/fixture","scenario":"fixed-record","reference_path":"fixture.png","reference_sha256":"a".repeat(64),"width":320,"height":240,"full_page":false,"mask_rectangles":[]}]});
    let CheckParams::VisualMatch(params) =
        parse_params("visual_match", 1, &body.to_string()).unwrap()
    else {
        panic!()
    };
    let temp = tempfile::tempdir().unwrap();
    let _ = check_visual_at(
        Path::new("/missing-visual-repo"),
        "missing",
        &params,
        &temp.path().join("missing.db"),
    );
}

#[test]
fn progress_scope_restores_after_nested_panic_and_does_not_spread_to_other_threads() {
    let outer_calls = Arc::new(Mutex::new(0));
    let count = outer_calls.clone();
    let outer: VisualProgressObserver = Arc::new(move |progress| {
        assert_eq!((progress.completed, progress.total), (0, 1));
        *count.lock().unwrap() += 1;
    });
    let inner_calls = Arc::new(Mutex::new(0));
    let count = inner_calls.clone();
    let inner: VisualProgressObserver = Arc::new(move |_| *count.lock().unwrap() += 1);
    with_visual_progress_observer(outer, || {
        unavailable_check();
        std::thread::spawn(unavailable_check).join().unwrap();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_visual_progress_observer(inner, || {
                unavailable_check();
                panic!("synthetic scope exit")
            });
        }));
        assert!(caught.is_err());
        unavailable_check();
    });
    unavailable_check();
    assert_eq!(*outer_calls.lock().unwrap(), 2);
    assert_eq!(*inner_calls.lock().unwrap(), 1);
}

#[test]
fn a_progress_callback_panic_does_not_change_the_checker_outcome() {
    with_visual_progress_observer(
        Arc::new(|_| panic!("progress consumer failed")),
        unavailable_check,
    );
}
