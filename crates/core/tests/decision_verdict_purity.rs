//! The verdict implementation has no model or HTTP provider import path.

use std::fs;
use std::path::Path;

fn rust_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = fs::read_dir(dir).expect("decisions source directory");
    for entry in entries {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn decisions_verdict_tree_has_no_model_or_http_provider_import() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/decisions");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(!files.is_empty());
    for path in files {
        let source = fs::read_to_string(&path).expect("read decisions source");
        for (line_number, line) in source.lines().enumerate() {
            let line = line.trim_start();
            if !line.starts_with("use ") && !line.starts_with("pub use ") {
                continue;
            }
            let import = line.split([';', ' ']).collect::<String>();
            for forbidden in ["mind", "language", "ureq", "reqwest"] {
                assert!(
                    !import
                        .split([':', '{', '}', ','])
                        .any(|name| name == forbidden),
                    "{}:{} imports {forbidden}",
                    path.display(),
                    line_number + 1
                );
            }
        }
    }
}
