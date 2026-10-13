//! Every new raw disk connection must be classified here. Live ledger opens
//! belong only to the hold-aware factory; other databases are named explicitly.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn rust_files(root: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(root).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            if path.file_name().and_then(|s| s.to_str()) != Some("tests") {
                rust_files(&path, out);
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
            && !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("_tests.rs")
        {
            out.push(path);
        }
    }
}

#[test]
fn raw_live_store_openers_exist_only_in_the_shared_hold_factory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // These are separate SQLite databases, never the mutable source ledger.
    // A new call even in an allowed file changes its count and fails review.
    let expected = BTreeMap::from([
        ("crates/core/src/store_hold.rs".to_owned(), 1), // the one live factory
        ("crates/core/src/codex_ingest.rs".to_owned(), 1), // disposable mirror index
        ("crates/core/src/generation_spool.rs".to_owned(), 1), // disposable parse spool
        ("crates/core/src/mirror.rs".to_owned(), 1),     // mirror coordination.sqlite lease
        ("crates/core/src/decisions/visual_inputs.rs".to_owned(), 1), // frozen C1 fixture
        ("crates/core/src/decisions/visual_record.rs".to_owned(), 2), // private C1 snapshot
        ("crates/ledger/src/schema.rs".to_owned(), 2),   // separate exec.db
    ]);
    let mut files = Vec::new();
    for path in ["crates/core/src", "crates/cli/src", "crates/ledger/src"] {
        rust_files(&root.join(path), &mut files);
    }
    let mut actual = BTreeMap::new();
    let mut attachments = BTreeMap::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("source text");
        let production = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap()
            // C48 protocol unit cases deliberately open a foreign tempfile
            // reader; this exact module is absent from every product build.
            .split("#[cfg(all(test, unix, debug_assertions))]\nmod protocol_tests")
            .next()
            .unwrap();
        let count = production
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                !line.starts_with("//")
                    && (line.contains("Connection::open(")
                        || line.contains("Connection::open_with_flags("))
            })
            .count();
        let attach_count = production
            .lines()
            .filter(|line| !line.trim_start().starts_with("//") && line.contains("ATTACH DATABASE"))
            .count();
        if attach_count > 0 {
            attachments.insert(
                path.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                attach_count,
            );
        }
        if count > 0 {
            actual.insert(
                path.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                count,
            );
        }
    }
    assert_eq!(actual, expected, "unclassified disk opener: route live ledgers through store_hold::open_connection; review other databases explicitly");
    assert_eq!(
        attachments,
        BTreeMap::from([("crates/core/src/store_hold.rs".to_owned(), 1)]),
        "source attachment requires the owner's read-only hold factory"
    );
}
