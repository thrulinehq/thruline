use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thruline_core::decisions::checks_visual::visual_process::capture_process;
#[test]
fn capture_progress_streams_before_the_child_finishes_and_ignores_callback_panic() {
    let started = Instant::now();
    let times = Arc::new(Mutex::new(Vec::new()));
    let copy = times.clone();
    let output=capture_process(Command::new("sh").args(["-c","printf 'Captured 1 of 2\n' >&2; sleep 0.15; printf 'Captured 2 of 2\n' >&2; printf '[]'"]),2,Some(Arc::new(move|p|{copy.lock().unwrap().push((p.completed,started.elapsed()));})),Duration::from_secs(3)).unwrap();
    assert!(output.status.success());
    let times = times.lock().unwrap();
    assert_eq!(times.len(), 2);
    assert!(times[0].1 < Duration::from_millis(125));
    assert!(times[1].1 >= Duration::from_millis(125));
    let output = capture_process(
        Command::new("sh").args(["-c", "printf 'Captured 1 of 1\n' >&2; printf '[]'"]),
        1,
        Some(Arc::new(|_| panic!("consumer"))),
        Duration::from_secs(3),
    )
    .unwrap();
    assert!(output.status.success());
}
#[test]
fn output_progress_and_deadlines_are_bounded() {
    assert!(capture_process(
        Command::new("sh").args(["-c", "printf 'Captured 2 of 1\n' >&2; printf '[]'"]),
        1,
        None,
        Duration::from_secs(3)
    )
    .is_err());
    let start = Instant::now();
    assert!(capture_process(
        Command::new("sh").args(["-c", "sleep 3"]),
        1,
        None,
        Duration::from_millis(50)
    )
    .is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}
