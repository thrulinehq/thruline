//! The collector's own suite is RUN, by `cargo test`, or it is not a guard.
//!
//! `crates/cli/src/verification_memory.py` is the only place in the product
//! where a wall clock decides whether a measurement exists. Its bound is now a
//! liveness bound (`SAMPLE_LIVENESS_SECONDS`), and the sample's own duration is
//! a field in the record (`sample_ms`) rather than a reason to refuse. The
//! facts behind that sentence — a slow `ps` still measures, a `ps` that never
//! answers still refuses, the emitted document carries both — are checked by
//! `verification_memory_sampling_test.py`, which needs a stubbed `ps` and so
//! cannot be written as a Rust integration test against the real collector.
//!
//! This test does **not** skip when `python3` is missing. `python3` is a hard
//! dependency of this collector already (it is the interpreter the check
//! spawns), so an unrunnable suite is an unchecked guard, and an unchecked
//! guard must not report green.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/cli -> repo root")
        .to_path_buf()
}

#[test]
fn the_memory_collector_sampling_suite_runs_and_passes() {
    let root = repo_root();
    let script = "crates/cli/tests/verification_memory_sampling_test.py";
    let path = root.join(script);
    assert!(
        path.is_file(),
        "{script} is gone. It is the suite that watches the memory collector's \
         sampling bound; deleting it is not a refactor."
    );
    let out = Command::new("python3")
        .arg(&path)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "could not run `python3 {script}`: {e}\n\
                 This test does NOT skip when the interpreter is missing: \
                 python3 is the interpreter the resource check itself spawns."
            )
        });
    assert!(
        out.status.success(),
        "{script} FAILED (exit {:?}) — the memory collector's sample is bounded \
         for speed again, or a slow sample is no longer a fact in the record.\n\
         \n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
