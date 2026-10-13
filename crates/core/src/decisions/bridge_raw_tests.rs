//! Private child fixtures reproduce metadata/open races; no person constructor.
use super::*;
use std::cell::RefCell;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::{fs::OpenOptions, io::Write, process::Command};
thread_local! {static MODE:RefCell<String>=const{RefCell::new(String::new())};}
fn mode() -> String {
    MODE.with(|v| v.borrow().clone())
}
fn replace(path: &Path, kind: &str) {
    fs::rename(path, path.with_extension("saved")).expect("fixture rename");
    match kind {
        "symlink" => {
            std::os::unix::fs::symlink(path.with_extension("saved"), path).expect("fixture symlink")
        }
        "fifo" => assert!(Command::new("python3")
            .arg("-c")
            .arg("import os,sys; os.mkfifo(sys.argv[1],0o600)")
            .arg(path)
            .status()
            .expect("fixture FIFO")
            .success()),
        "regular" => {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)
                .expect("fixture replacement");
            file.write_all(b"{}\n").expect("fixture bytes");
        }
        _ => panic!("unknown fixture replacement"),
    }
}
pub(super) fn before_open(path: &Path) {
    match mode().as_str() {
        "race_fifo" => replace(path, "fifo"),
        "race_symlink" => replace(path, "symlink"),
        "race_regular" => replace(path, "regular"),
        _ => (),
    }
}
pub(super) fn after_open(path: &Path, file: &fs::File) {
    match mode().as_str() {
        "after_symlink" => replace(path, "symlink"),
        "after_regular" => replace(path, "regular"),
        "after_write" => fs::write(path, b"changed after open\n").expect("fixture edit"),
        "flags" => assert!(rustix::fs::fcntl_getfl(file)
            .expect("descriptor flags")
            .contains(rustix::fs::OFlags::NONBLOCK)),
        _ => (),
    }
}
const HARNESS: &str = r#"
import subprocess,sys,time,tempfile,os,shutil
exe,mode=sys.argv[1:]
root=tempfile.mkdtemp(prefix="thruline-c35-raw-")
started=time.monotonic()
try:
    result=subprocess.run([exe,'--exact','decisions::authority::bridge::raw_receipt::tests::raw_child','--ignored','--nocapture','--test-threads=1'],env={**os.environ,'THRULINE_RAW_UNIT_MODE':mode,'THRULINE_RAW_UNIT_ROOT':root},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=2)
    print(result.stdout.decode(errors='replace'))
    print('raw fixture '+mode+' elapsed='+str(time.monotonic()-started)+' exit='+str(result.returncode))
    sys.exit(result.returncode)
except subprocess.TimeoutExpired as error:
    print((error.stdout or b'').decode(errors='replace'))
    print('raw fixture '+mode+' blocked past 2s; subprocess.run killed and reaped the known fixture child')
    sys.exit(1)
finally:
    shutil.rmtree(root)
"#;
fn run(mode: &str) {
    let output = Command::new("python3")
        .arg("-c")
        .arg(HARNESS)
        .arg(std::env::current_exe().expect("unit binary"))
        .arg(mode)
        .output()
        .expect("raw fixture rig");
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn ordinary_private_regular_receipt_still_reads() {
    run("regular");
}
#[test]
fn raw_reader_refuses_static_symlink() {
    run("symlink");
}
#[test]
fn raw_reader_refuses_static_fifo() {
    run("fifo");
}
#[test]
fn raw_reader_fifo_swap_cannot_block_the_read() {
    run("race_fifo");
}
#[test]
fn raw_reader_refuses_symlink_swap_before_open() {
    run("race_symlink");
}
#[test]
fn raw_reader_refuses_regular_swap_before_open() {
    run("race_regular");
}
#[test]
fn raw_reader_refuses_symlink_swap_after_open() {
    run("after_symlink");
}
#[test]
fn raw_reader_refuses_regular_swap_after_open() {
    run("after_regular");
}
#[test]
fn raw_reader_refuses_in_place_change_after_open() {
    run("after_write");
}
#[test]
fn raw_reader_descriptor_is_nonblocking() {
    run("flags");
}
#[test]
fn raw_reader_retains_full_file_size_bound() {
    run("oversized");
}
#[test]
fn raw_reader_refuses_expired_budget_before_read() {
    run("expired");
}
#[test]
#[ignore = "private raw reader child; called by parent fixtures"]
fn raw_child() {
    let mode = std::env::var("THRULINE_RAW_UNIT_MODE").expect("private fixture mode");
    MODE.with(|v| *v.borrow_mut() = mode.clone());
    let directory = PathBuf::from(
        std::env::var("THRULINE_RAW_UNIT_ROOT").expect("parent-owned fixture directory"),
    );
    let path = directory.join("receipt.jsonl");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .expect("fixture file");
    file.write_all(b"{}\n").expect("fixture line");
    drop(file);
    let uid = fs::metadata(&path).expect("fixture metadata").uid();
    if mode == "symlink" {
        replace(&path, "symlink");
    }
    if mode == "fifo" {
        replace(&path, "fifo");
    }
    if mode == "oversized" {
        fs::write(&path, vec![b'a'; LIMIT + 1]).expect("fixture oversize");
    }
    let deadline = if mode == "expired" {
        Instant::now() - Duration::from_secs(1)
    } else {
        Instant::now() + Duration::from_secs(1)
    };
    let result = read_private(&path, uid, Some(deadline));
    if mode == "regular" || mode == "flags" {
        assert_eq!(result, Some(b"{}\n".to_vec()));
    } else {
        assert!(
            result.is_none(),
            "{mode} must refuse descriptor or named-path change"
        );
    }
}
