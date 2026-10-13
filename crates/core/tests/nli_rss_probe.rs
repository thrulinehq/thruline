//! M3.2b — sequence-length / RSS probe (A25).
//!
//! Two phases, deliberately in **separate processes**, because the question is
//! scorer-alone peak RSS and derivation state would contaminate it. That split
//! is also a rehearsal of §5a's non-co-residency requirement.
//!
//! ```text
//! # 1. census: real premise token lengths, tokenizer only, no ONNX session
//! cargo test --release --test nli_rss_probe -- --ignored --nocapture census
//!
//! # 2. RSS at a cap: scorer only, premises read from the census file
//! THRULINE_MAX_TOKENS=512 /usr/bin/time -l <bin> --ignored --nocapture rss_at_cap
//! ```
//!
//! Both are `#[ignore]`d: they need the real ledger and the downloaded model,
//! neither of which a fresh clone has.

//! Requires the `ort` feature: this file exercises the local ONNX
//! entailment backend, which `--no-default-features` removes. Compiled out
//! rather than skipped, so a feature-off build has nothing to link.
#![cfg(feature = "ort")]

use std::path::PathBuf;

use thruline_core::ledger::Ledger;
use thruline_core::nli::{EntailmentScorer, OrtNli};
use thruline_core::premise;

/// Where the census writes premises for the RSS phase to read back.
fn premise_file() -> PathBuf {
    std::env::var_os("THRULINE_PROBE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("thruline-probe-premises.txt"))
}

#[path = "support/live_store.rs"]
mod live_store;

fn ledger_path() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("HOME")?).join(".thruline/ledger.db");
    if p.exists() {
        live_store::refuse_live_store(&p);
    }
    p.exists().then_some(p)
}

fn model_dir() -> Option<PathBuf> {
    let d = OrtNli::default_dir().ok()?;
    d.join("model_quantized.onnx").exists().then_some(d)
}

/// Profile the exact pairs already frozen in `evidence_links`.
///
/// Unlike `census`, this keeps each claim's real hypothesis. It is the bounded
/// diagnostic used by A65 to distinguish a long-sequence workload from query
/// or cache collapse without re-running annotation.
#[test]
#[ignore]
fn actual_pair_census() {
    let (Some(db), Some(dir)) = (ledger_path(), model_dir()) else {
        eprintln!("SKIP: needs the real ledger and the downloaded model");
        return;
    };
    let project = std::env::var("THRULINE_PROBE_PROJECT")
        .expect("THRULINE_PROBE_PROJECT names one already-annotated project");
    let tok = tokenizers::Tokenizer::from_file(dir.join("tokenizer.json")).expect("tokenizer");
    let ledger = Ledger::open(&db).expect("open ledger");

    let mut stmt = ledger
        .conn()
        .prepare(
            "SELECT c.text, l.evidence_event_id
             FROM evidence_links l
             JOIN claims c ON c.id = l.claim_id
             JOIN events e ON e.id = c.event_id
             WHERE e.project_id = ?1
             ORDER BY c.id, l.evidence_event_id",
        )
        .expect("pair query");
    let pairs: Vec<(String, String)> = stmt
        .query_map([&project], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query pairs")
        .collect::<Result<_, _>>()
        .expect("read pairs");
    drop(stmt);

    let mut lens = Vec::with_capacity(pairs.len());
    let started = std::time::Instant::now();
    for (claim, event_id) in &pairs {
        let row = thruline_core::derive::evidence_row_for(&ledger, event_id)
            .expect("evidence query")
            .expect("linked evidence exists");
        let premise = premise::render(&row);
        let len = tok
            .encode((premise.as_str(), claim.as_str()), true)
            .expect("encode")
            .get_ids()
            .len();
        lens.push(len);
    }
    lens.sort_unstable();
    let pct = |q: f64| -> usize {
        if lens.is_empty() {
            return 0;
        }
        lens[((lens.len() as f64 - 1.0) * q).round() as usize]
    };
    let over = |cap: usize| lens.iter().filter(|&&l| l > cap).count();
    eprintln!(
        "{project}: pairs={} read+tokenize={:?} p50={} p90={} p99={} max={}",
        lens.len(),
        started.elapsed(),
        pct(0.50),
        pct(0.90),
        pct(0.99),
        lens.last().copied().unwrap_or(0)
    );
    for cap in [512usize, 256, 192] {
        eprintln!(
            "cap {cap:>3}: {} of {} pairs truncated ({:.2}%)",
            over(cap),
            lens.len(),
            100.0 * over(cap) as f64 / lens.len().max(1) as f64
        );
    }
}

/// Short, fresh-process throughput sample over the exact frozen pairs.
///
/// Reports fixed-size buckets so a long project can reveal within-process
/// decay separately from corpus shape. It is intentionally bounded by
/// `THRULINE_PROBE_N`; profiling must never become another full annotation.
#[test]
#[ignore]
fn actual_pair_throughput() {
    let (Some(db), Some(dir)) = (ledger_path(), model_dir()) else {
        eprintln!("SKIP: needs the real ledger and the downloaded model");
        return;
    };
    let project = std::env::var("THRULINE_PROBE_PROJECT")
        .expect("THRULINE_PROBE_PROJECT names one already-annotated project");
    let limit: usize = std::env::var("THRULINE_PROBE_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3_000);
    let bucket = 500usize;
    let ledger = Ledger::open(&db).expect("open ledger");
    let mut stmt = ledger
        .conn()
        .prepare(
            "SELECT c.text, l.evidence_event_id
             FROM evidence_links l
             JOIN claims c ON c.id = l.claim_id
             JOIN events e ON e.id = c.event_id
             WHERE e.project_id = ?1
             ORDER BY c.id, l.evidence_event_id
             LIMIT ?2",
        )
        .expect("pair query");
    let pairs: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![project, limit as i64], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("query pairs")
        .collect::<Result<_, _>>()
        .expect("read pairs");
    drop(stmt);

    let mut nli = OrtNli::open(&dir).expect("load scorer");
    let total_started = std::time::Instant::now();
    let mut bucket_started = std::time::Instant::now();
    for (i, (claim, event_id)) in pairs.iter().enumerate() {
        let row = thruline_core::derive::evidence_row_for(&ledger, event_id)
            .expect("evidence query")
            .expect("linked evidence exists");
        let rendered = premise::render(&row);
        nli.score(&rendered, claim).expect("score");
        if (i + 1) % bucket == 0 || i + 1 == pairs.len() {
            let count = if (i + 1) % bucket == 0 {
                bucket
            } else {
                (i + 1) % bucket
            };
            let elapsed = bucket_started.elapsed();
            eprintln!(
                "pairs {:>5}-{:>5}: {:.1}/sec",
                i + 2 - count,
                i + 1,
                count as f64 / elapsed.as_secs_f64()
            );
            bucket_started = std::time::Instant::now();
        }
    }
    eprintln!(
        "{project}: {} exact pairs in {:?} = {:.1}/sec",
        pairs.len(),
        total_started.elapsed(),
        pairs.len() as f64 / total_started.elapsed().as_secs_f64()
    );
}

/// M4.R0: replay the exact production rendering path, not the shorter base
/// premise used by `actual_pair_throughput`. This is deliberately bounded:
/// it diagnoses the 90k-pair run without becoming another afternoon-long run.
#[test]
#[ignore]
fn actual_production_pair_throughput() {
    let (Some(db), Some(dir)) = (ledger_path(), model_dir()) else {
        eprintln!("SKIP: needs the real ledger and the downloaded model");
        return;
    };
    let project = std::env::var("THRULINE_PROBE_PROJECT")
        .expect("THRULINE_PROBE_PROJECT names one already-annotated project");
    let limit: usize = std::env::var("THRULINE_PROBE_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3_000);
    let bucket = 500usize;
    let ledger = Ledger::open(&db).expect("open ledger");
    let mut stmt = ledger
        .conn()
        .prepare(
            "SELECT c.text, l.evidence_event_id
             FROM evidence_links l
             JOIN claims c ON c.id = l.claim_id
             JOIN events e ON e.id = c.event_id
             WHERE e.project_id = ?1
             ORDER BY c.id, l.evidence_event_id
             LIMIT ?2",
        )
        .expect("pair query");
    let pairs: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![project, limit as i64], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("query pairs")
        .collect::<Result<_, _>>()
        .expect("read pairs");
    drop(stmt);

    let mut nli = OrtNli::open(&dir).expect("load scorer");
    let total_started = std::time::Instant::now();
    let mut bucket_started = std::time::Instant::now();
    let mut bucket_tokens = Vec::with_capacity(bucket);
    let mut bucket_work = std::time::Duration::ZERO;
    let mut prepare = std::time::Duration::ZERO;
    let mut inference = std::time::Duration::ZERO;
    let mut token_measure = std::time::Duration::ZERO;
    for (i, (claim, event_id)) in pairs.iter().enumerate() {
        let prepare_started = std::time::Instant::now();
        let row = thruline_core::derive::evidence_row_for(&ledger, event_id)
            .expect("evidence query")
            .expect("linked evidence exists");
        let detail =
            thruline_core::derive::evidence_detail_for(&ledger, event_id).expect("evidence detail");
        let rendered = premise::render_tagged_for_claim(&row, claim, detail.as_deref());
        let prepared = prepare_started.elapsed();
        prepare += prepared;
        let token_started = std::time::Instant::now();
        let tokens = nli.token_len(&rendered.text, claim).expect("token length");
        token_measure += token_started.elapsed();

        let inference_started = std::time::Instant::now();
        nli.score(&rendered.text, claim).expect("score");
        let inferred = inference_started.elapsed();
        inference += inferred;
        bucket_work += prepared + inferred;
        bucket_tokens.push(tokens);

        if (i + 1) % bucket == 0 || i + 1 == pairs.len() {
            let count = bucket_tokens.len();
            bucket_tokens.sort_unstable();
            let p50 = bucket_tokens[count / 2];
            let p90 = bucket_tokens[((count - 1) as f64 * 0.9).round() as usize];
            let diagnostic_wall = bucket_started.elapsed();
            eprintln!(
                "production pairs {:>5}-{:>5}: {:.1}/sec excluding token census · tokens p50/p90 {p50}/{p90} · diagnostic wall {:?}",
                i + 2 - count,
                i + 1,
                count as f64 / bucket_work.as_secs_f64(),
                diagnostic_wall,
            );
            bucket_tokens.clear();
            bucket_work = std::time::Duration::ZERO;
            bucket_started = std::time::Instant::now();
        }
    }
    let elapsed = total_started.elapsed();
    eprintln!(
        "{project}: {} production-rendered pairs · estimated production {:.1}/sec · diagnostic wall {:?} · prepare {:?} · inference {:?} · token census {:?}",
        pairs.len(),
        pairs.len() as f64 / (prepare + inference).as_secs_f64(),
        elapsed,
        prepare,
        inference,
        token_measure,
    );
}

/// Phase 1. Render every evidence row of the named projects through the
/// production renderer, tokenize each against a representative claim, and
/// report the length distribution. No ONNX session is created here.
#[test]
#[ignore]
fn census() {
    let (Some(db), Some(dir)) = (ledger_path(), model_dir()) else {
        eprintln!("SKIP: needs the real ledger and the downloaded model");
        return;
    };
    let projects: Vec<String> = std::env::var("THRULINE_PROBE_PROJECTS")
        .unwrap_or_else(|_| {
            "-Users-dev-thruline,-Users-dev-Documents-ATLAS,\
             -Users-dev-Developer-market"
                .to_string()
        })
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Tokenizer only — loading the ONNX session here would put 400+ MB
    // alongside derivation and measure the wrong thing.
    let tok = tokenizers::Tokenizer::from_file(dir.join("tokenizer.json")).expect("tokenizer");

    let ledger = Ledger::open(&db).expect("open ledger");
    let mut premises: Vec<String> = Vec::new();
    for p in &projects {
        let g = thruline_core::derive::derive_project(&ledger, p).expect("derive");
        let mut n = 0usize;
        for node in &g.nodes {
            for row in &node.evidence {
                premises.push(premise::render(row));
                n += 1;
            }
        }
        eprintln!("{p}: {} nodes, {n} evidence rows", g.nodes.len());
    }

    // A representative claim; the hypothesis is short and near-constant in
    // length, so the distribution is dominated by the premise.
    const CLAIM: &str = "I've updated the module and all the tests pass.";
    let mut lens: Vec<usize> = premises
        .iter()
        .map(|p| {
            tok.encode((p.as_str(), CLAIM), true)
                .expect("encode")
                .get_ids()
                .len()
        })
        .collect();
    lens.sort_unstable();
    let pct = |q: f64| -> usize {
        if lens.is_empty() {
            return 0;
        }
        let i = ((lens.len() as f64 - 1.0) * q).round() as usize;
        lens[i]
    };
    let over = |cap: usize| lens.iter().filter(|&&l| l > cap).count();
    eprintln!("\n--- premise+claim token lengths, n={} ---", lens.len());
    eprintln!(
        "p50 {}  p90 {}  p99 {}  max {}",
        pct(0.50),
        pct(0.90),
        pct(0.99),
        lens.last().copied().unwrap_or(0)
    );
    for cap in [512usize, 256, 192] {
        eprintln!(
            "cap {cap:>3}: {} of {} pairs truncated ({:.2}%)",
            over(cap),
            lens.len(),
            100.0 * over(cap) as f64 / lens.len().max(1) as f64
        );
    }

    let out = premise_file();
    // One premise per line; the renderer never emits a newline.
    let body: String = premises
        .iter()
        .map(|p| p.replace('\n', " "))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&out, body).expect("write premises");
    eprintln!("wrote {} premises to {}", premises.len(), out.display());
}

/// Phase 2. Scorer only. Run under `/usr/bin/time -l` for peak RSS.
#[test]
#[ignore]
fn rss_at_cap() {
    let Some(dir) = model_dir() else {
        eprintln!("SKIP: no model");
        return;
    };
    let cap: usize = std::env::var("THRULINE_MAX_TOKENS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(thruline_core::nli::MAX_TOKENS);
    let file = premise_file();
    let text = std::fs::read_to_string(&file).unwrap_or_else(|_| {
        panic!(
            "no premise census at {} — run the `census` test first",
            file.display()
        )
    });
    // Longest-first by default, so the cap is actually exercised rather than
    // sampled away. `THRULINE_PROBE_ORDER=asis` keeps corpus order instead,
    // which is the honest input for a *throughput* number — the longest-first
    // ordering deliberately measures the worst case, not the typical one.
    let mut premises: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if std::env::var("THRULINE_PROBE_ORDER").as_deref() != Ok("asis") {
        premises.sort_by_key(|p| std::cmp::Reverse(p.len()));
    }
    let n: usize = std::env::var("THRULINE_PROBE_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    premises.truncate(n.max(1));

    let mut nli = OrtNli::open_with(&dir, cap).expect("load");
    const CLAIM: &str = "I've updated the module and all the tests pass.";
    let t = std::time::Instant::now();
    for p in &premises {
        nli.score(p, CLAIM).expect("score");
    }
    let el = t.elapsed();
    eprintln!(
        "cap={cap} pairs={} truncated={} wall={:?} pairs/sec={:.1}",
        nli.pairs_scored(),
        nli.pairs_truncated(),
        el,
        nli.pairs_scored() as f64 / el.as_secs_f64()
    );
}
