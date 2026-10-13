//! C20: every shipped CLI and book sentence is free of trial offers.
//!
//! This deliberately scans the source that can produce customer-facing CLI
//! output, the public release README, and the book. UI source, copy goldens,
//! and the embedded build are covered by `ui/src/workspace/no-trial.test.js`.
//! Historical decision documents are records of earlier rulings and are not
//! current product copy. Unrelated uses outside these surfaces are explicitly
//! limited to the benchmark's run-count vocabulary in
//! `crates/bench/src/{driver,pilot,tau2}.rs` and the payment-provider fixture
//! `crates/licence-issue/fixtures/paddle-transaction_completed.json`.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn gather(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        files.push(path.to_path_buf());
        return;
    }
    for entry in std::fs::read_dir(path).expect("read copy surface") {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if path.is_dir() {
            gather(&path, files);
        } else if matches!(path.extension().and_then(|s| s.to_str()), Some("rs" | "md")) {
            files.push(path);
        }
    }
}

#[test]
fn no_trial_word_reaches_shipped_cli_or_reading_copy() {
    let root = repo();
    let mut files = Vec::new();
    for path in [
        "crates/licence/src",
        "crates/cli/src",
        "docs/book/src",
        "packaging/public-release/README.md",
    ] {
        gather(&root.join(path), &mut files);
    }
    files.sort();
    assert!(files.len() > 20, "the census did not reach its surfaces");

    let mut matches = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("read copy surface");
        for (line, content) in text.lines().enumerate() {
            if content.to_ascii_lowercase().contains("trial") {
                let shown = path.strip_prefix(&root).expect("relative path");
                matches.push(format!(
                    "{}:{}: {}",
                    shown.display(),
                    line + 1,
                    content.trim()
                ));
            }
        }
    }
    assert!(
        matches.is_empty(),
        "trial copy remains:\n{}",
        matches.join("\n")
    );
}
