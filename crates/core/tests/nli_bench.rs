//! Throughput / RSS bench. Not a gate — `#[ignore]`d so `cargo test` stays
//! fast — but committed so the M3.2 numbers stay reproducible:
//!
//! ```text
//! cargo test --release --test nli_bench -- --ignored --nocapture
//! ```
//!
//! Measured on this machine (Apple silicon, CPU only, single intra-thread):
//! model load 315 ms · **146 pairs/sec** · **peak RSS 478 MB**.
//! Requires the `ort` feature: this file exercises the local ONNX
//! entailment backend, which `--no-default-features` removes. Compiled out
//! rather than skipped, so a feature-off build has nothing to link.
#![cfg(feature = "ort")]

use std::time::Instant;
use thruline_core::nli::{EntailmentScorer, OrtNli};

#[test]
#[ignore]
fn throughput_and_rss() {
    let dir = OrtNli::default_dir().expect("home");
    if !dir.join("model_quantized.onnx").exists() {
        return;
    }
    let t_load = Instant::now();
    let mut nli = OrtNli::open(&dir).expect("load");
    println!("model load: {:?}", t_load.elapsed());

    // Representative premises: the shapes the renderer actually emits.
    let premises: Vec<String> = vec![
        "The command cargo test completed successfully. All 214 tests passed.",
        "The file src/auth/jwt.rs was edited. The new content includes: fn verify(token: &str) -> Result<Claims> { let key = load_key()?; decode(token, &key) }",
        "The command npm run build was run. Its output ended with: built in 312ms",
        "The tool Read was run src/lib.rs. It returned: pub mod chain; pub mod config; pub mod derive;",
        "The command cargo clippy completed successfully.",
        "The file docs/plan.md was created. The new content includes: # Plan Step one Step two",
        "The command git commit -m wip was run. Its output ended with: 3 files changed, 40 insertions(+)",
        "The command ./deploy.sh was run and it failed.",
    ].into_iter().map(String::from).collect();
    let claims = [
        "All 214 tests pass.",
        "I've updated the JWT verification.",
        "The build succeeded.",
        "I've added the config module.",
    ];

    // Warm up (first inference allocates arenas).
    for _ in 0..8 {
        nli.score(&premises[0], claims[0]).expect("warm");
    }

    let n = 400usize;
    let t = Instant::now();
    for i in 0..n {
        let p = &premises[i % premises.len()];
        let c = claims[i % claims.len()];
        nli.score(p, c).expect("score");
    }
    let el = t.elapsed();
    println!(
        "pairs: {n}  wall: {:?}  pairs/sec: {:.1}",
        el,
        n as f64 / el.as_secs_f64()
    );
    println!("total pairs scored by this session: {}", nli.pairs_scored());
}
