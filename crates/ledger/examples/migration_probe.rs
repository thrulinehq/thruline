//! Times `Ledger::open`'s migration against a real store, for the figure the
//! performance registry requires (`docs/perf-basis.toml`).
//!
//! An example rather than a test: it needs a store of a stated size, and a test
//! that silently skips when one is absent is a measurement nobody takes.
//!
//! Usage: `cargo run -p thruline-ledger --example migration_probe -- <path>`
fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: migration_probe <path to a copy of exec.db>");
        std::process::exit(2);
    };
    let path = std::path::PathBuf::from(path);
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let started = std::time::Instant::now();
    let ledger = match thruline_ledger::Ledger::open(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("open failed: {e}");
            std::process::exit(1);
        }
    };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let version = ledger.schema_version().unwrap_or(0);
    println!("bytes={bytes} open_and_migrate_ms={ms:.1} schema_version={version}");
}
