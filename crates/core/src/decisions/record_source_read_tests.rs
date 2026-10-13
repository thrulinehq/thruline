//! Deterministic replacements of authored temporary sources, never live records.
//! This module and its hooks are absent from production builds.
use super::*;
use std::cell::RefCell;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;

type Hook = Option<Box<dyn FnOnce()>>;
thread_local! {
    static BEFORE_OPEN: RefCell<Hook> = RefCell::new(None);
    static AFTER_OPEN: RefCell<Hook> = RefCell::new(None);
}

pub(super) fn before_open() {
    BEFORE_OPEN.with(|hook| {
        if let Some(callback) = hook.borrow_mut().take() {
            callback();
        }
    });
}

pub(super) fn after_open() {
    AFTER_OPEN.with(|hook| {
        if let Some(callback) = hook.borrow_mut().take() {
            callback();
        }
    });
}

fn fifo(path: &Path) {
    assert!(Command::new("mkfifo")
        .args(["-m", "600"])
        .arg(path)
        .status()
        .unwrap()
        .success());
}

fn run_case(case: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "decisions::record_source::read_tests::source_read_worker",
            "--nocapture",
        ])
        .env("THRULINE_DECISION_SOURCE_READ_CASE", case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        if child.try_wait().unwrap().is_some() {
            let result = child.wait_with_output().unwrap();
            assert!(
                result.status.success(),
                "{case}: {} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        if Instant::now() >= until {
            child.kill().unwrap();
            let result = child.wait_with_output().unwrap();
            panic!(
                "{case}: source read blocked beyond isolated watchdog: {} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn source_read_worker() {
    let Ok(case) = std::env::var("THRULINE_DECISION_SOURCE_READ_CASE") else {
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.jsonl");
    fs::write(&path, b"first\nsecond\n").unwrap();
    let mut want = 1;
    let mut expected = None;
    match case.as_str() {
        "stationary-fifo" => {
            fs::remove_file(&path).unwrap();
            fifo(&path);
        }
        "stationary-symlink" => {
            let target = temp.path().join("fifo");
            fifo(&target);
            fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(target, &path).unwrap();
        }
        "before-fifo" => {
            let path = path.clone();
            BEFORE_OPEN.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    fs::remove_file(&path).unwrap();
                    fifo(&path);
                }))
            });
        }
        "before-symlink" => {
            let target = temp.path().join("fifo");
            fifo(&target);
            let path = path.clone();
            BEFORE_OPEN.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    fs::remove_file(&path).unwrap();
                    std::os::unix::fs::symlink(target, &path).unwrap();
                }))
            });
        }
        "before-regular" | "after-regular" => {
            let replacement = temp.path().join("replacement");
            fs::write(&replacement, b"first\nsecond\n").unwrap();
            let path = path.clone();
            let callback: Hook = Some(Box::new(move || fs::rename(replacement, path).unwrap()));
            if case == "before-regular" {
                BEFORE_OPEN.with(|hook| *hook.borrow_mut() = callback);
            } else {
                AFTER_OPEN.with(|hook| *hook.borrow_mut() = callback);
            }
        }
        "after-growth" => {
            let path = path.clone();
            AFTER_OPEN.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    OpenOptions::new()
                        .append(true)
                        .open(path)
                        .unwrap()
                        .write_all(b"changed\n")
                        .unwrap();
                }))
            });
        }
        "after-modified" => {
            let path = path.clone();
            AFTER_OPEN.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    fs::write(&path, b"other\nsecond\n").unwrap();
                    OpenOptions::new()
                        .write(true)
                        .open(path)
                        .unwrap()
                        .set_times(
                            std::fs::FileTimes::new()
                                .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(10)),
                        )
                        .unwrap();
                }))
            });
        }
        "oversized-line" => fs::write(&path, vec![b'x'; MAX_RAW_BYTES + 3]).unwrap(),
        "missing-line" => want = 10,
        "lf" => {
            want = 2;
            expected = Some(vec![b"first".to_vec(), b"second".to_vec()]);
        }
        "crlf" => {
            fs::write(&path, b"first\r\nsecond\r\n").unwrap();
            want = 2;
            expected = Some(vec![b"first".to_vec(), b"second".to_vec()]);
        }
        "final-line" => {
            fs::write(&path, b"first\nsecond").unwrap();
            want = 2;
            expected = Some(vec![b"first".to_vec(), b"second".to_vec()]);
        }
        "large-stable-tail" => {
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(MAX_READ_BYTES as u64 + 1)
                .unwrap();
            expected = Some(vec![b"first".to_vec()]);
        }
        _ => panic!("unknown authored fixture case"),
    }
    let result = read_raw_prefix(&path, want, Instant::now() + Duration::from_secs(1)).unwrap();
    assert_eq!(result, expected, "{case}");
}

#[test]
fn source_descriptor_rejects_stationary_fifo() {
    run_case("stationary-fifo");
}
#[test]
fn source_descriptor_rejects_stationary_symlink() {
    run_case("stationary-symlink");
}
#[test]
fn source_descriptor_rejects_fifo_replacement_before_open() {
    run_case("before-fifo");
}
#[test]
fn source_descriptor_rejects_symlink_replacement_before_open() {
    run_case("before-symlink");
}
#[test]
fn source_descriptor_rejects_regular_replacement_before_open() {
    run_case("before-regular");
}
#[test]
fn source_descriptor_rejects_regular_replacement_after_open() {
    run_case("after-regular");
}
#[test]
fn source_descriptor_rejects_growth_after_open() {
    run_case("after-growth");
}
#[test]
fn source_descriptor_rejects_same_size_content_change_after_open() {
    run_case("after-modified");
}
#[test]
fn source_descriptor_keeps_lf_crlf_and_final_prefixes() {
    for case in ["lf", "crlf", "final-line"] {
        run_case(case);
    }
}
#[test]
fn source_descriptor_keeps_bounded_prefix_of_large_stable_session() {
    run_case("large-stable-tail");
}
#[test]
fn source_descriptor_rejects_oversized_line() {
    run_case("oversized-line");
}
#[test]
fn source_descriptor_rejects_missing_pinned_line() {
    run_case("missing-line");
}
