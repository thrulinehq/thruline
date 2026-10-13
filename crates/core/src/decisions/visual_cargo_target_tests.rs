use super::*;
use std::{
    fs,
    time::{Duration, Instant},
};
const COMMIT: &str = "b942b5db18633909ceaf60f8c9fc314d3ca8df30";
#[test]
fn default_route_retains_only_the_legacy_commit_namespace() {
    let temp = tempfile::tempdir().unwrap();
    let route = CargoTarget::resolve(COMMIT, None, temp.path()).unwrap();
    assert_eq!(
        route.target,
        temp.path().join("thruline-c1-cargo-target").join(COMMIT)
    );
}
#[test]
fn explicit_existing_root_routes_distinct_immutable_commits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("external-target");
    fs::create_dir(&root).unwrap();
    let route = CargoTarget::resolve(COMMIT, Some(&root), temp.path()).unwrap();
    assert_eq!(route.target, root.join(COMMIT));
    let other = "0123456789abcdef0123456789abcdef01234567";
    assert_ne!(
        route.target,
        CargoTarget::resolve(other, Some(&root), temp.path())
            .unwrap()
            .target
    );
}
#[test]
fn unavailable_relative_file_or_symlink_root_is_refused_without_recreation() {
    let temp = tempfile::tempdir().unwrap();
    let absent = temp.path().join("disconnected");
    assert!(CargoTarget::resolve(COMMIT, Some(&absent), temp.path()).is_err());
    assert!(!absent.exists());
    assert!(CargoTarget::resolve(COMMIT, Some(Path::new("relative")), temp.path()).is_err());
    let file = temp.path().join("file");
    fs::write(&file, "fixture").unwrap();
    assert!(CargoTarget::resolve(COMMIT, Some(&file), temp.path()).is_err());
    #[cfg(unix)]
    {
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(temp.path(), &link).unwrap();
        assert!(CargoTarget::resolve(COMMIT, Some(&link), temp.path()).is_err());
    }
}
#[test]
fn mutable_abbreviated_or_path_commit_namespace_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    for commit in [
        "",
        "HEAD",
        "efdc063e",
        "../escape",
        "/absolute",
        "gggggggggggggggggggggggggggggggggggggggg",
    ] {
        assert!(
            CargoTarget::resolve(commit, None, temp.path()).is_err(),
            "{commit}"
        );
    }
}
#[test]
fn disappearing_or_replaced_target_root_invalidates_its_pin() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("external");
    fs::create_dir(&root).unwrap();
    let route = CargoTarget::resolve(COMMIT, Some(&root), temp.path()).unwrap();
    fs::rename(&root, temp.path().join("old-root")).unwrap();
    assert!(route.verify().is_err());
    fs::create_dir(&root).unwrap();
    assert!(route.verify().is_err());
}
#[test]
fn capacity_gates_use_target_bytes_and_each_other_compiler_reserve() {
    assert!(check_space(25 * GIB, 0, true).is_ok());
    assert!(check_space(25 * GIB - 1, 0, true).is_err());
    assert!(check_space(35 * GIB - 1, 1, true).is_err());
    assert!(check_space(35 * GIB, 1, true).is_ok());
    assert!(check_space(45 * GIB, 2, true).is_ok());
    assert!(check_space(15 * GIB, 10, false).is_ok());
    assert!(check_space(15 * GIB - 1, 0, false).is_err());
}
#[test]
fn failed_start_guard_does_not_spawn_any_child() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("started");
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "printf started > \"$1\"", "fixture"])
        .arg(&marker);
    assert!(monitored_output(&mut cmd, || Err("target unavailable")).is_err());
    assert!(!marker.exists());
}
#[test]
fn disappearing_target_stops_and_reaps_the_child_group() {
    let temp = tempfile::tempdir().unwrap();
    let pid = temp.path().join("child.pid");
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "echo $$ > \"$1\"; sleep 1", "fixture"])
        .arg(&pid);
    let start = Instant::now();
    let mut checks = 0;
    let result = monitored_output(&mut cmd, || {
        checks += 1;
        if checks >= 3 {
            Err("target disappeared")
        } else {
            Ok(())
        }
    });
    assert_eq!(result.unwrap_err(), "target disappeared");
    assert!(start.elapsed() < Duration::from_secs(2));
    if let Ok(text) = fs::read_to_string(pid) {
        assert!(!Command::new("kill")
            .args(["-0", text.trim()])
            .status()
            .unwrap()
            .success());
    }
}
#[test]
fn nested_command_disables_debug_and_incremental_and_keeps_target_separate() {
    let temp = tempfile::tempdir().unwrap();
    let route = CargoTarget::resolve(COMMIT, None, temp.path()).unwrap();
    let mut cmd = Command::new("sh");
    for name in [
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_TEST_DEBUG",
        "CARGO_PROFILE_RELEASE_DEBUG",
    ] {
        cmd.env(name, "2");
    }
    cmd.env("CARGO_INCREMENTAL", "1");
    cmd.args(["-c","printf '%s|%s|%s|%s|%s' \"$CARGO_PROFILE_DEV_DEBUG\" \"$CARGO_PROFILE_TEST_DEBUG\" \"$CARGO_PROFILE_RELEASE_DEBUG\" \"$CARGO_INCREMENTAL\" \"$CARGO_TARGET_DIR\""]);
    route.configure(&mut cmd);
    let out = monitored_output(&mut cmd, || Ok(())).unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("0|0|0|0|{}", route.target.display())
    );
}

#[test]
fn compiler_census_counts_roots_not_their_rustc_descendants() {
    assert_eq!(compiler_roots("1 0 /sbin/launchd\n2 1 /bin/cargo\n3 2 /bin/sh\n4 3 /bin/rustc\n5 1 /bin/cargo-clippy\n6 5 /bin/rustc\n7 1 /bin/rustc\n").unwrap(),3);
    assert!(compiler_roots("").is_err());
    assert!(compiler_roots("invalid census").is_err());
}

#[test]
fn shared_start_lock_is_existing_regular_nonsymlink_and_exclusive() {
    let temp = tempfile::tempdir().unwrap();
    let absent = temp.path().join("absent");
    assert!(start_lock(Some(&absent)).is_err());
    assert!(!absent.exists());
    let lock_path = temp.path().join("start.lock");
    fs::write(&lock_path, "").unwrap();
    #[cfg(unix)]
    {
        let link = temp.path().join("alias.lock");
        std::os::unix::fs::symlink(&lock_path, &link).unwrap();
        assert!(start_lock(Some(&link)).is_err());
        let held = start_lock(Some(&lock_path)).unwrap();
        let second = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        assert!(rustix::fs::flock(
            &second,
            rustix::fs::FlockOperation::NonBlockingLockExclusive
        )
        .is_err());
        drop(held);
        assert!(rustix::fs::flock(
            &second,
            rustix::fs::FlockOperation::NonBlockingLockExclusive
        )
        .is_ok());
    }
}

#[test]
fn pinned_commit_namespace_disappearance_or_replacement_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mut route = CargoTarget::resolve(COMMIT, Some(temp.path()), temp.path()).unwrap();
    fs::create_dir(&route.target).unwrap();
    route.namespace_pin = Some(DirectoryPin::open(&route.target).unwrap());
    fs::rename(&route.target, temp.path().join("old-namespace")).unwrap();
    assert!(route.verify().is_err());
    fs::create_dir(&route.target).unwrap();
    assert!(route.verify().is_err());
}

#[test]
fn normal_parent_exit_stops_descendants_before_reaping_and_finishes_output() {
    let temp = tempfile::tempdir().unwrap();
    let pid = temp.path().join("descendant.pid");
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "sleep 20 & echo $! > \"$1\"; printf done", "fixture"])
        .arg(&pid);
    let before = Instant::now();
    let out = monitored_output(&mut cmd, || Ok(())).unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"done");
    assert!(before.elapsed() < Duration::from_secs(2));
    let pid = fs::read_to_string(pid).unwrap();
    assert!(!Command::new("kill")
        .args(["-0", pid.trim()])
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
fn namespace_creation_uses_the_pinned_directory_not_a_replacement_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let pin = DirectoryPin::open(&root).unwrap();
    let old = temp.path().join("original-root");
    fs::rename(&root, &old).unwrap();
    fs::create_dir(&root).unwrap();
    create_under(&pin, std::ffi::OsStr::new(COMMIT)).unwrap();
    assert!(old.join(COMMIT).is_dir());
    assert!(!root.join(COMMIT).exists());
    assert!(pin.verify().is_err());
}

#[test]
#[ignore = "C50: visual cargo census dropped for v0.2 (C44 §8)"]
fn native_kernel_capacity_and_process_census_are_available_on_this_host() {
    let temp = tempfile::tempdir().unwrap();
    let pin = DirectoryPin::open(temp.path()).unwrap();
    assert!(pin.free().unwrap() > 0);
    // The outer guarded Cargo test invocation is itself a compiler root.
    assert!(active_compilers().unwrap() >= 1);
}
