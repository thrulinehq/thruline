//! RUN-STOP-PGID-GUARD: every process-group signal in product code goes
//! through the one guarded door, `thruline_core::bounded_child::signal_group`.
//!
//! `rustix::process::kill_process_group(pid, …)` sends `kill(-pid, …)`. Handed
//! a recorded pgid of 1 it becomes `kill(-1, …)` — every process the user may
//! signal. The guard refuses 0, 1, negative and HQ's own group before any
//! signal; this walk makes sure no product file calls past it. The one file
//! allowed to name the raw call is the guard's own module, where the real
//! signaller sits behind the refusal.

use std::path::{Path, PathBuf};

const RAW_CALL: &str = "kill_process_group(";
const THE_GUARD: &str = "core/src/bounded_child.rs";

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_product_file_signals_a_process_group_past_the_guard() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ is the manifest's parent")
        .to_path_buf();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates)
        .expect("crates/ is readable")
        .flatten()
    {
        rust_files(&entry.path().join("src"), &mut files);
    }
    assert!(
        files.len() > 50,
        "the walk must reach the workspace's sources, found {}",
        files.len()
    );

    let mut unguarded = Vec::new();
    for file in &files {
        let rel = file
            .strip_prefix(&crates)
            .expect("under crates/")
            .to_string_lossy()
            .replace('\\', "/");
        // Test-only trees signal groups they spawned themselves.
        if rel == THE_GUARD || rel.contains("/tests/") {
            continue;
        }
        let text = std::fs::read_to_string(file).expect("a source file is readable");
        for (n, line) in text.lines().enumerate() {
            if line.contains(RAW_CALL) {
                unguarded.push(format!("{rel}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        unguarded.is_empty(),
        "process-group signals that bypass the guard:\n{}",
        unguarded.join("\n")
    );
}
