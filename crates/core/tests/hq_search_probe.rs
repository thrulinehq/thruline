//! What the record search costs on a synthetic store of realistic size.
//! Ignored by default: it builds a store of thousands of sessions and
//! measures, it does not assert. Never a real store.
//!
//! `HQ_SEARCH_SESSIONS=2000 cargo test --release -p thruline-core --test hq_search_probe -- --ignored --nocapture`

mod scratch;

use std::time::{Duration, Instant};

use scratch::TempTree;
use serde_json::json;
use thruline_core::hq;
use thruline_core::hq::search::Corpus;
use thruline_core::ingest::ingest;
use thruline_core::Ledger;

const ROOT: &str = "/work/big";

/// A small deterministic generator (xorshift), so every run builds the same store.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

const WORDS: &[&str] = &[
    "fix",
    "the",
    "checkout",
    "total",
    "discount",
    "parser",
    "cache",
    "retry",
    "login",
    "token",
    "session",
    "schema",
    "query",
    "render",
    "layout",
    "button",
    "export",
    "import",
    "config",
    "build",
    "deploy",
    "logging",
    "metrics",
    "worker",
    "queue",
    "invoice",
    "refund",
    "search",
    "index",
    "filter",
    "migration",
    "upgrade",
    "timeout",
    "latency",
    "error",
    "handler",
    "route",
];

fn sentence(r: &mut Rng, words: usize) -> String {
    (0..words)
        .map(|_| WORDS[r.below(WORDS.len() as u64) as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

fn path(r: &mut Rng) -> String {
    let dirs = [
        "src",
        "src/api",
        "src/cart",
        "lib",
        "tests",
        "docs",
        "app/models",
        "app/views",
    ];
    format!(
        "{}/{}_{}.py",
        dirs[r.below(dirs.len() as u64) as usize],
        WORDS[r.below(WORDS.len() as u64) as usize],
        r.below(400)
    )
}

fn write_session(dir: &std::path::Path, n: usize, r: &mut Rng) -> usize {
    let id = format!("cccccccc-{:04x}-4000-8000-{:012x}", n % 0xffff, n);
    let day = 1 + n / 200;
    let mut lines = Vec::new();
    let mut k = 0u64;
    let mut line = |kind: &str, message: serde_json::Value| {
        k += 1;
        let secs = k * 7;
        json!({
            "parentUuid": null, "isSidechain": false, "uuid": format!("{id}-{k:05}"),
            "timestamp": format!("2026-{:02}-{:02}T{:02}:{:02}:{:02}.000Z",
                1 + (day / 28) % 12, 1 + day % 28, (n % 12) + (secs / 3600) as usize % 12, (secs / 60) % 60, secs % 60),
            "cwd": ROOT, "sessionId": id, "version": "2.1.0", "userType": "external",
            "type": kind, "message": message,
        })
        .to_string()
    };
    for turn in 0..5 {
        let len = 20 + r.below(200) as usize;
        lines.push(line("user", json!({"role": "user", "content": format!("Please {} in {}.", sentence(r, len), path(r))})));
        let call = |name: &str,
                    input: serde_json::Value,
                    result: String,
                    lines: &mut Vec<String>,
                    line: &mut dyn FnMut(&str, serde_json::Value) -> String| {
            let tu = format!("{id}-t{}", lines.len());
            lines.push(line("assistant", json!({"role": "assistant", "model": "m", "content": [{"type": "tool_use", "id": tu, "name": name, "input": input}]})));
            lines.push(line("user", json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": tu, "content": result, "is_error": false}]})));
        };
        for _ in 0..6 {
            let p = path(r);
            call(
                "Read",
                json!({"file_path": format!("{ROOT}/{p}")}),
                sentence(r, 30),
                &mut lines,
                &mut line,
            );
        }
        for _ in 0..3 {
            let p = path(r);
            call(
                "Edit",
                json!({"file_path": format!("{ROOT}/{p}"), "old_string": "a", "new_string": "b"}),
                "ok".into(),
                &mut lines,
                &mut line,
            );
        }
        let t = path(r);
        call(
            "Bash",
            json!({"command": format!("pytest -q tests/{}", t.rsplit('/').next().unwrap_or("x"))}),
            format!("{} passed in 0.5s", 1 + r.below(50)),
            &mut lines,
            &mut line,
        );
        call(
            "Bash",
            json!({"command": format!("grep -rn {} src", WORDS[r.below(WORDS.len() as u64) as usize])}),
            sentence(r, 40),
            &mut lines,
            &mut line,
        );
        if turn == 4 {
            let hash = format!("{:07x}", r.next() & 0xfff_ffff);
            call(
                "Bash",
                json!({"command": format!("git commit -m \"{}\"", sentence(r, 6))}),
                format!("[main {hash}] done\n 3 files changed"),
                &mut lines,
                &mut line,
            );
        }
        lines.push(line("assistant", json!({"role": "assistant", "model": "m", "content": [{"type": "text", "text": format!("Done. All {} tests pass and the {} is fixed.", 1 + r.below(50), WORDS[r.below(WORDS.len() as u64) as usize])}]})));
    }
    let rows = lines.len();
    std::fs::write(dir.join(format!("{id}.jsonl")), lines.join("\n") + "\n").expect("write");
    rows
}

fn stats(mut xs: Vec<Duration>) -> String {
    xs.sort();
    format!(
        "min {:?} median {:?} max {:?}",
        xs[0],
        xs[xs.len() / 2],
        xs[xs.len() - 1]
    )
}

#[test]
#[ignore = "measurement on a synthetic store; run with --release --ignored --nocapture"]
fn what_a_search_costs_on_thousands_of_sessions() {
    let sessions: usize = std::env::var("HQ_SEARCH_SESSIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let tmp = TempTree::new();
    let dir = tmp.path().join("claude/-work-big");
    std::fs::create_dir_all(&dir).expect("dir");
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let t = Instant::now();
    let rows: usize = (0..sessions).map(|n| write_session(&dir, n, &mut r)).sum();
    eprintln!(
        "generated {sessions} sessions, {rows} transcript lines in {:?}",
        t.elapsed()
    );
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    let t = Instant::now();
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    eprintln!("ingested in {:?}", t.elapsed());
    let events: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |x| x.get(0))
        .expect("count");
    let project = hq::hq_projects(&ledger).expect("projects").remove(0);
    drop(ledger);

    let ledger = Ledger::open(&db).expect("open");
    let t = Instant::now();
    let index = hq::index(&ledger, &project).expect("index");
    let index_took = t.elapsed();
    let threads = index.view(0).threads.len();
    let mut builds = Vec::new();
    let mut corpus = None;
    for _ in 0..3 {
        let t = Instant::now();
        corpus = Some(Corpus::build(&ledger, &index).expect("corpus"));
        builds.push(t.elapsed());
    }
    let corpus = corpus.expect("built");
    eprintln!(
        "store: {events} events, {threads} threads; index {index_took:?}; corpus {} things, {} bytes of text; build {}",
        corpus.len(),
        corpus.text_bytes(),
        stats(builds)
    );
    let open = index.view(0).threads.first().map(|t| t.id.clone());
    for q in [
        "checkout",
        "src/cart/refund_17.py",
        "migration timeout",
        "was a migration run?",
        "zzzzqqqq",
        "a",
        "e",
    ] {
        for thread in [None, open.as_deref()] {
            let mut took = Vec::new();
            let mut doc = None;
            for _ in 0..7 {
                let t = Instant::now();
                let d = corpus.search(q, 20, thread);
                let bytes = serde_json::to_vec(&d).expect("json").len();
                took.push(t.elapsed());
                doc = Some((d.total, d.results.len(), bytes));
            }
            let (total, served, bytes) = doc.expect("searched");
            eprintln!(
                "search {q:?} open={}: total {total}, served {served}, {bytes} bytes JSON; {}",
                thread.is_some(),
                stats(took)
            );
        }
    }
}
