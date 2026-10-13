use std::path::Path;

fn rust_sources(path: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).expect("Decisions source directory") {
        let entry = entry.expect("source entry");
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn verdict_modules_import_no_model_or_network_client() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/decisions");
    let mut sources = Vec::new();
    rust_sources(&root, &mut sources);
    assert!(!sources.is_empty());
    let forbidden = ["mind", "language", "ureq", "reqwest", "ollama", "openai"];
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("source text");
        for (line_number, line) in source.lines().enumerate() {
            let line = line.trim_start();
            if !(line.starts_with("use ") || line.starts_with("extern crate ")) {
                continue;
            }
            let lower = line.to_ascii_lowercase();
            for name in forbidden {
                assert!(
                    !lower.contains(name),
                    "model or network import in {}:{}: {line}",
                    path.display(),
                    line_number + 1,
                );
            }
        }
    }
}

#[test]
fn proposer_has_no_acceptance_writer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/decisions/grammar.rs");
    let source = std::fs::read_to_string(path).expect("proposal reader");
    assert!(!source.contains("use super::storage"));
    assert!(!source.contains("use crate::decisions::storage"));
    assert!(!source.contains("pub fn accept("));
}
